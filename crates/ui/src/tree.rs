use std::any::Any;
use std::collections::BTreeSet;
use std::marker::PhantomData;

use crate::element::{Color, Element, TextSize};

/// An element in a `Tree`. Ids aren't reused, so one never names a later element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(u32);

/// A UI tree, and the signals and effects that change it.
pub struct Tree {
    nodes: Vec<Option<Node>>,
    changes: Changes,
    signals: Vec<Box<dyn Any + Send + Sync>>,
    /// The effects that read each signal.
    readers: Vec<Vec<usize>>,
    /// `None` while the effect runs.
    effects: Vec<Option<Effect>>,
}

type Effect = Box<dyn FnMut(&mut Tree) + Send + Sync>;

struct Node {
    element: Element,
    parent: Option<Id>,
    children: Vec<Id>,
}

/// What changed since the last `take_changes`, for drawing only that.
#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    /// New elements, and elements whose fields, or inherited text properties, changed.
    pub updated: BTreeSet<Id>,
    /// Removed elements, descendants included.
    pub removed: Vec<Id>,
    /// Elements whose children were added to or removed.
    pub reordered: BTreeSet<Id>,
}

/// A value effects can read and re-run on. A handle: copy it freely.
pub struct Signal<T> {
    index: usize,
    value: PhantomData<fn() -> T>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Signal<T> {}

impl<T> Signal<T> {
    pub fn id(self) -> SignalId {
        SignalId(self.index)
    }
}

/// A signal of any type, for listing what an effect reads.
#[derive(Clone, Copy, Debug)]
pub struct SignalId(usize);

impl Tree {
    pub fn new(root: Element) -> Tree {
        let mut tree = Tree {
            nodes: Vec::new(),
            changes: Changes::default(),
            signals: Vec::new(),
            readers: Vec::new(),
            effects: Vec::new(),
        };
        tree.insert(root, None);
        tree
    }

    pub fn root(&self) -> Id {
        Id(0)
    }

    pub fn append(&mut self, parent: Id, element: Element) -> Id {
        let id = self.insert(element, Some(parent));
        self.node_mut(parent).children.push(id);
        self.changes.reordered.insert(parent);
        id
    }

    fn insert(&mut self, element: Element, parent: Option<Id>) -> Id {
        let id = Id(self.nodes.len() as u32);
        self.nodes.push(Some(Node {
            element,
            parent,
            children: Vec::new(),
        }));
        self.changes.updated.insert(id);
        id
    }

    /// Removes the element and its descendants. The root stays.
    pub fn remove(&mut self, id: Id) {
        let Some(parent) = self.node(id).parent else {
            return;
        };
        self.node_mut(parent).children.retain(|&c| c != id);
        self.changes.reordered.insert(parent);
        for removed in self.descendants(id) {
            self.nodes[removed.0 as usize] = None;
            self.changes.updated.remove(&removed);
            self.changes.reordered.remove(&removed);
            self.changes.removed.push(removed);
        }
    }

    pub fn contains(&self, id: Id) -> bool {
        self.nodes.get(id.0 as usize).is_some_and(Option::is_some)
    }

    pub fn element(&self, id: Id) -> &Element {
        &self.node(id).element
    }

    pub fn parent(&self, id: Id) -> Option<Id> {
        self.node(id).parent
    }

    pub fn children(&self, id: Id) -> &[Id] {
        &self.node(id).children
    }

    pub fn update(&mut self, id: Id, change: impl FnOnce(&mut Element)) {
        let element = &mut self.node_mut(id).element;
        let inherited = (element.color, element.font_size);
        change(element);
        if inherited == (element.color, element.font_size) {
            self.changes.updated.insert(id);
        } else {
            let all = self.descendants(id);
            self.changes.updated.extend(all);
        }
    }

    /// The element's text color: its own, else its nearest ancestor's.
    pub fn color(&self, id: Id) -> Color {
        self.inherited(id, |e| e.color).unwrap_or(Color::Foreground)
    }

    pub fn font_size(&self, id: Id) -> TextSize {
        self.inherited(id, |e| e.font_size).unwrap_or(TextSize::M)
    }

    fn inherited<T>(&self, id: Id, field: impl Fn(&Element) -> Option<T>) -> Option<T> {
        let mut at = Some(id);
        while let Some(id) = at {
            if let Some(value) = field(self.element(id)) {
                return Some(value);
            }
            at = self.parent(id);
        }
        None
    }

    pub fn take_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }

    pub fn signal<T: Send + Sync + 'static>(&mut self, value: T) -> Signal<T> {
        self.signals.push(Box::new(value));
        self.readers.push(Vec::new());
        Signal {
            index: self.signals.len() - 1,
            value: PhantomData,
        }
    }

    pub fn get<T: 'static>(&self, signal: Signal<T>) -> &T {
        self.signals[signal.index]
            .downcast_ref()
            .expect("a signal's handle has its value's type")
    }

    /// Sets the value and re-runs the effects that read the signal.
    pub fn set<T: Send + Sync + 'static>(&mut self, signal: Signal<T>, value: T) {
        self.signals[signal.index] = Box::new(value);
        // A copy, since an effect that runs can add effects reading this signal.
        for effect in self.readers[signal.index].clone() {
            self.run(effect);
        }
    }

    /// Runs `effect` now, and again whenever a signal in `reads` is set.
    pub fn effect(
        &mut self,
        reads: &[SignalId],
        effect: impl FnMut(&mut Tree) + Send + Sync + 'static,
    ) {
        let index = self.effects.len();
        self.effects.push(Some(Box::new(effect)));
        for read in reads {
            self.readers[read.0].push(index);
        }
        self.run(index);
    }

    /// Keeps one field of an element set from a signal, until the element is removed.
    pub fn bind<T: Clone + Send + Sync + 'static>(
        &mut self,
        id: Id,
        signal: Signal<T>,
        set: fn(&mut Element, T),
    ) {
        self.effect(&[signal.id()], move |tree| {
            if tree.contains(id) {
                let value = tree.get(signal).clone();
                tree.update(id, |element| set(element, value));
            }
        });
    }

    fn run(&mut self, index: usize) {
        // An effect that sets a signal it reads isn't run again inside itself.
        if let Some(mut effect) = self.effects[index].take() {
            effect(self);
            self.effects[index] = Some(effect);
        }
    }

    fn descendants(&self, id: Id) -> Vec<Id> {
        let mut all = vec![id];
        let mut i = 0;
        while let Some(&at) = all.get(i) {
            all.extend_from_slice(self.children(at));
            i += 1;
        }
        all
    }

    fn node(&self, id: Id) -> &Node {
        self.nodes[id.0 as usize]
            .as_ref()
            .expect("an id names an element still in its tree")
    }

    fn node_mut(&mut self, id: Id) -> &mut Node {
        self.nodes[id.0 as usize]
            .as_mut()
            .expect("an id names an element still in its tree")
    }
}
