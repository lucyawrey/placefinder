use ui::{Color, Kind, ReadError, Space, Tree, read};

fn kinds(tree: &Tree, id: ui::Id) -> Vec<Kind> {
    tree.children(id)
        .iter()
        .map(|&c| tree.element(c).kind)
        .collect()
}

#[test]
fn a_cml_file_loads_into_the_tree() {
    let tree = read(
        "page.cml",
        "<Box padding=M background=Rgb(1, 2, 3)>
          <Text color=Accent><Span>Go to </Span><Link target=\"meadow\">Meadow</Link></Text>
          <Button>Save</Button>
          <TextInput value=\"x\"/>
          <Image src=\"art/a.png\" width=Px(40)/>
        </Box>",
    )
    .unwrap();
    let root = tree.root();
    assert_eq!(tree.element(root).padding, Some(Space::M));
    assert_eq!(tree.element(root).background, Some(Color::Rgb(1, 2, 3)));
    use Kind::*;
    assert_eq!(kinds(&tree, root), [Text, Button, TextInput, Image]);
    let line = tree.children(root)[0];
    assert_eq!(kinds(&tree, line), [Span, Link]);
    let [span, link] = tree.children(line) else {
        panic!()
    };
    assert_eq!(tree.element(*span).text, "Go to ");
    assert_eq!(tree.element(*link).target, "meadow");
    assert_eq!(tree.color(*link), Color::Accent);
    assert_eq!(tree.element(tree.children(root)[1]).text, "Save");
}

#[test]
fn a_wrong_field_or_unknown_token_is_refused_with_checks_error() {
    for (page, code) in [
        ("<Box colour=Accent/>", "wrong_arguments"),
        ("<Box padding=Huge/>", "unknown_name"),
    ] {
        match read("page.cml", page) {
            Err(ReadError::Invalid(e)) => assert!(e.contains(code), "{page}: {e}"),
            other => panic!("{page}: {:?}", other.map(|_| ())),
        }
    }
}

#[test]
fn spans_go_only_in_text_and_text_holds_only_spans_and_links() {
    for page in [
        "<Box><Span>a</Span></Box>",
        "<Text><Box/></Text>",
        "<Span>a</Span>",
    ] {
        assert!(
            matches!(read("page.cml", page), Err(ReadError::Misplaced(_))),
            "{page}"
        );
    }
}

#[test]
fn an_image_outside_the_places_folder_is_refused() {
    assert!(read("page.cml", "<Image src=\"a/../b.png\"/>").is_ok());
    for src in ["../b.png", "a/../../b.png"] {
        assert_eq!(
            read("page.cml", &format!("<Image src=\"{src}\"/>")).err(),
            Some(ReadError::OutsidePlace(src.into()))
        );
    }
}
