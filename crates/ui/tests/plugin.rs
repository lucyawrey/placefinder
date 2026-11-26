use bevy::prelude::*;
use ui::{Element, Kind, Tree, UiPlugin, UiRoot};

fn texts(app: &mut App, root: Entity) -> Vec<String> {
    let world = app.world_mut();
    let top = world.get::<Children>(root).unwrap()[0];
    world.get::<Children>(top).map_or(vec![], |c| {
        c.iter()
            .map(|e| world.get::<Text>(e).unwrap().0.clone())
            .collect()
    })
}

#[test]
fn the_bevy_nodes_follow_the_tree() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), UiPlugin));
    let mut tree = Tree::new(Element::new(Kind::Box));
    let top = tree.root();
    let a = tree.append(
        top,
        Element {
            text: "a".into(),
            ..Element::new(Kind::Text)
        },
    );
    let b = tree.append(
        top,
        Element {
            text: "b".into(),
            ..Element::new(Kind::Button)
        },
    );
    let root = app
        .world_mut()
        .spawn((
            Node::default(),
            UiRoot {
                tree,
                assets: String::new(),
            },
        ))
        .id();
    app.update();
    assert_eq!(texts(&mut app, root), ["a", "b"]);

    let mut ui = app.world_mut().get_mut::<UiRoot>(root).unwrap();
    ui.tree.update(a, |e| e.text = "c".into());
    ui.tree.remove(b);
    ui.tree.append(
        top,
        Element {
            text: "d".into(),
            ..Element::new(Kind::Link)
        },
    );
    app.update();
    assert_eq!(texts(&mut app, root), ["c", "d"]);
}
