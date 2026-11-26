use std::collections::BTreeSet;

use ui::{Changes, Color, Element, Kind, Space, TextSize, Tree};

fn text(words: &str) -> Element {
    Element {
        text: words.into(),
        ..Element::new(Kind::Text)
    }
}

#[test]
fn builds_changes_and_removes_elements_reporting_each_change() {
    let mut tree = Tree::new(Element::new(Kind::Box));
    let root = tree.root();
    let panel = tree.append(root, Element::new(Kind::Box));
    let label = tree.append(panel, text("a"));
    assert_eq!(tree.children(panel), [label]);
    assert_eq!(
        tree.take_changes(),
        Changes {
            updated: BTreeSet::from([root, panel, label]),
            removed: vec![],
            reordered: BTreeSet::from([root, panel]),
        }
    );

    tree.update(label, |e| e.text = "b".into());
    assert_eq!(tree.element(label).text, "b");
    assert_eq!(tree.take_changes().updated, BTreeSet::from([label]));

    tree.remove(panel);
    assert!(!tree.contains(label));
    assert_eq!(tree.children(root), []);
    assert_eq!(
        tree.take_changes(),
        Changes {
            updated: BTreeSet::new(),
            removed: vec![panel, label],
            reordered: BTreeSet::from([root]),
        }
    );
}

#[test]
fn text_properties_inherit_and_others_dont() {
    let mut tree = Tree::new(Element {
        color: Some(Color::Accent),
        padding: Some(Space::L),
        ..Element::new(Kind::Box)
    });
    let root = tree.root();
    let label = tree.append(root, text("a"));
    assert_eq!(tree.color(label), Color::Accent);
    assert_eq!(tree.font_size(label), TextSize::M);
    assert_eq!(tree.element(label).padding, None);
    tree.take_changes();

    tree.update(root, |e| e.font_size = Some(TextSize::Xl));
    assert_eq!(tree.font_size(label), TextSize::Xl);
    assert_eq!(tree.take_changes().updated, BTreeSet::from([root, label]));
}

#[test]
fn tokens_resolve_to_values_and_numbers_pass_through() {
    assert!(Space::S.px() < Space::M.px() && Space::M.px() < Space::L.px());
    assert_eq!(Space::Px(12.0).px(), 12.0);
    assert_eq!(TextSize::Px(9.0).px(), 9.0);
    assert_eq!(Color::Rgb(255, 0, 51).rgba(), [1.0, 0.0, 0.2, 1.0]);
    assert_eq!(Color::Clear.rgba()[3], 0.0);
}

#[test]
fn a_bound_field_follows_its_signal_and_nothing_else_changes() {
    let mut tree = Tree::new(Element::new(Kind::Box));
    let root = tree.root();
    let label = tree.append(root, text(""));
    let other = tree.append(root, text("other"));
    let name = tree.signal(String::from("a"));
    tree.bind(label, name, |e, words| e.text = words);
    assert_eq!(tree.element(label).text, "a");
    tree.take_changes();

    tree.set(name, "b".into());
    assert_eq!(tree.element(label).text, "b");
    assert_eq!(tree.element(other).text, "other");
    assert_eq!(tree.take_changes().updated, BTreeSet::from([label]));

    tree.remove(label);
    tree.set(name, "c".into());
}

#[test]
fn an_effect_reruns_when_a_signal_it_reads_is_set() {
    let mut tree = Tree::new(Element::new(Kind::Box));
    let count = tree.signal(1);
    let doubled = tree.signal(0);
    tree.effect(&[count.id()], move |t| {
        let n = *t.get(count);
        t.set(doubled, n * 2);
    });
    assert_eq!(*tree.get(doubled), 2);
    tree.set(count, 5);
    assert_eq!(*tree.get(doubled), 10);
}
