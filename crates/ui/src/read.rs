use std::collections::HashMap;
use std::fmt;

use check_core::modules::MemFiles;
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, VariantAccess, Visitor,
};
use serde::{Deserialize, Deserializer};

use crate::element::{Element, Kind};
use crate::tree::{Id, Tree};

const CLASS: &str = include_str!("ui.check");

#[derive(Debug, PartialEq)]
pub enum ReadError {
    /// Check's error, which it formats for people.
    Invalid(String),
    /// A `Span` outside a `Text`, or a `Text` holding something other than spans and links.
    Misplaced(Kind),
    /// An image whose `src` climbs out of the Place's folder.
    OutsidePlace(String),
}

/// Reads a `.cml` file, checked against `ui.check`, into a tree.
pub fn read(path: &str, text: &str) -> Result<Tree, ReadError> {
    let files = MemFiles(HashMap::from([("ui/ui.check".into(), CLASS.into())]));
    let (root, _): (Read, _) =
        check_serde::from_str_with_class(&files, "ui/", "ui", path, text, "ui.Element")
            .map_err(|e| ReadError::Invalid(e.to_string()))?;
    check(&root, None)?;
    let mut tree = Tree::new(root.element);
    let id = tree.root();
    append(&mut tree, id, root.children);
    Ok(tree)
}

fn check(read: &Read, parent: Option<Kind>) -> Result<(), ReadError> {
    let kind = read.element.kind;
    let inline = matches!(kind, Kind::Span | Kind::Link);
    if (kind == Kind::Span && parent != Some(Kind::Text)) || (parent == Some(Kind::Text) && !inline)
    {
        return Err(ReadError::Misplaced(kind));
    }
    if kind == Kind::Image && leaves_folder(&read.element.src) {
        return Err(ReadError::OutsidePlace(read.element.src.clone()));
    }
    read.children.iter().try_for_each(|c| check(c, Some(kind)))
}

fn leaves_folder(path: &str) -> bool {
    let mut depth = 0;
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if depth == 0 => return true,
            ".." => depth -= 1,
            _ => depth += 1,
        }
    }
    false
}

fn append(tree: &mut Tree, parent: Id, children: Vec<Read>) {
    for child in children {
        let id = tree.append(parent, child.element);
        append(tree, id, child.children);
    }
}

/// An element as read, before it joins a tree.
struct Read {
    element: Element,
    children: Vec<Read>,
}

/// Each element class reads into the one `Element` struct, keeping its class as the kind.
impl<'de> Deserialize<'de> for Read {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Read, D::Error> {
        deserializer.deserialize_enum("Element", &[], ElementVisitor)
    }
}

struct ElementVisitor;

impl<'de> Visitor<'de> for ElementVisitor {
    type Value = Read;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an element")
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<Read, A::Error> {
        let (kind, fields) = data.variant::<Kind>()?;
        let mut read = fields.struct_variant(&[], FieldsVisitor)?;
        read.element.kind = kind;
        Ok(read)
    }
}

struct FieldsVisitor;

impl<'de> Visitor<'de> for FieldsVisitor {
    type Value = Read;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an element's fields")
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Read, A::Error> {
        let mut fields = WithoutChildren {
            map,
            children: Vec::new(),
        };
        let element = Element::deserialize(de::value::MapAccessDeserializer::new(&mut fields))?;
        Ok(Read {
            element,
            children: fields.children,
        })
    }
}

/// Passes an element's fields through to `Element`, keeping its children aside.
struct WithoutChildren<A> {
    map: A,
    children: Vec<Read>,
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for WithoutChildren<A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, A::Error> {
        while let Some(key) = self.map.next_key::<String>()? {
            if key != "children" {
                return seed.deserialize(key.into_deserializer()).map(Some);
            }
            self.children = self.map.next_value()?;
        }
        Ok(None)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, A::Error> {
        self.map.next_value_seed(seed)
    }
}
