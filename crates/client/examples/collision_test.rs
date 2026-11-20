//! Writes `places/collision-test/scene.gltf` and `scene.bin`: plain shapes for trying out walking.

use bevy::math::{Quat, Vec3};
use rmcp::serde_json::{Value, json};

/// Flat-shaded triangles: three positions and one normal per triangle.
#[derive(Default)]
struct Shape {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
}

impl Shape {
    fn quad(mut self, corners: [Vec3; 4]) -> Self {
        let normal = (corners[1] - corners[0])
            .cross(corners[2] - corners[0])
            .normalize();
        for i in [0, 1, 2, 0, 2, 3] {
            self.positions.push(corners[i]);
            self.normals.push(normal);
        }
        self
    }

    fn cuboid(mut self, center: Vec3, half: Vec3) -> Self {
        for (axis, sign) in [
            (0, 1.0),
            (0, -1.0),
            (1, 1.0),
            (1, -1.0),
            (2, 1.0),
            (2, -1.0),
        ] {
            let corner = |a: f32, b: f32| {
                let mut p = [0.0; 3];
                p[axis] = sign;
                p[(axis + 1) % 3] = a * sign;
                p[(axis + 2) % 3] = b;
                center + half * Vec3::from_array(p)
            };
            self = self.quad([
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ]);
        }
        self
    }

    fn block(self, min: Vec3, max: Vec3) -> Self {
        self.cuboid((min + max) / 2.0, (max - min) / 2.0)
    }
}

struct Node {
    name: &'static str,
    shape: Option<Shape>,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
    children: Vec<Node>,
}

fn node(name: &'static str, shape: Shape) -> Node {
    Node {
        name,
        shape: Some(shape),
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
        children: Vec::new(),
    }
}

fn scene() -> Vec<Node> {
    let mut floor = Shape::default();
    for x in -6..6 {
        for z in -6..6 {
            let (x, z) = (x as f32 * 2.0, z as f32 * 2.0);
            floor = floor.quad([
                Vec3::new(x, 0.0, z),
                Vec3::new(x, 0.0, z + 2.0),
                Vec3::new(x + 2.0, 0.0, z + 2.0),
                Vec3::new(x + 2.0, 0.0, z),
            ]);
        }
    }

    let mut stairs = Shape::default();
    for step in 0..5 {
        let top = 0.25 * (step + 1) as f32;
        let near = 2.0 - 0.4 * step as f32;
        stairs = stairs.block(Vec3::new(-1.0, 0.0, near - 0.4), Vec3::new(1.0, top, near));
    }

    let mut room = Shape::default();
    for (min, max) in [
        (Vec3::new(-10.0, 0.0, 4.0), Vec3::new(-9.8, 3.0, 10.0)),
        (Vec3::new(-10.0, 0.0, 4.0), Vec3::new(-4.0, 3.0, 4.2)),
        (Vec3::new(-10.0, 0.0, 9.8), Vec3::new(-4.0, 3.0, 10.0)),
        (Vec3::new(-4.2, 0.0, 4.0), Vec3::new(-4.0, 3.0, 6.4)),
        (Vec3::new(-4.2, 0.0, 7.6), Vec3::new(-4.0, 3.0, 10.0)),
    ] {
        room = room.block(min, max);
    }

    let tilted = |name, degrees: f32, x: f32| Node {
        translation: Vec3::new(x, 0.0, 0.0),
        rotation: Quat::from_rotation_x(degrees.to_radians()),
        ..node(
            name,
            Shape::default().cuboid(Vec3::new(0.0, -0.5, -3.0), Vec3::new(1.0, 0.5, 3.0)),
        )
    };

    let mut rotated = node("rotated box parent", Shape::default());
    rotated.shape = None;
    rotated.translation = Vec3::new(8.0, 0.0, 2.0);
    rotated.children.push(Node {
        translation: Vec3::new(0.0, 1.0, 0.0),
        rotation: Quat::from_rotation_y(45f32.to_radians()),
        scale: Vec3::new(2.0, 1.0, 1.0),
        ..node(
            "rotated box",
            Shape::default().cuboid(Vec3::ZERO, Vec3::ONE),
        )
    });

    vec![
        node("floor", floor),
        node("stairs", stairs),
        node(
            "platform",
            Shape::default().block(Vec3::new(-1.5, 0.0, -3.0), Vec3::new(1.5, 1.25, 0.0)),
        ),
        node(
            "tall step",
            Shape::default().block(Vec3::new(3.0, 0.0, 4.0), Vec3::new(5.0, 0.5, 6.0)),
        ),
        tilted("gentle ramp", 20.0, -6.0),
        tilted("steep ramp", 70.0, -9.5),
        node(
            "doorway",
            Shape::default()
                .block(Vec3::new(2.0, 0.0, -6.2), Vec3::new(5.4, 3.0, -6.0))
                .block(Vec3::new(6.6, 0.0, -6.2), Vec3::new(10.0, 3.0, -6.0))
                .block(Vec3::new(5.4, 2.2, -6.2), Vec3::new(6.6, 3.0, -6.0)),
        ),
        node("hollow room", room),
        rotated,
    ]
}

/// Adds a node and its children, returning its index.
fn write_node(node: Node, gltf: &mut Value, buffer: &mut Vec<u8>) -> usize {
    let mut entry = json!({
        "name": node.name,
        "translation": node.translation.to_array(),
        "rotation": node.rotation.to_array(),
        "scale": node.scale.to_array(),
    });
    if let Some(shape) = node.shape {
        let position = add_accessor(&shape.positions, true, gltf, buffer);
        let normal = add_accessor(&shape.normals, false, gltf, buffer);
        let meshes = gltf["meshes"].as_array_mut().unwrap();
        meshes.push(json!({
            "name": node.name,
            "primitives": [{ "attributes": { "POSITION": position, "NORMAL": normal }, "material": 0 }],
        }));
        entry["mesh"] = json!(meshes.len() - 1);
    }
    let children: Vec<usize> = node
        .children
        .into_iter()
        .map(|child| write_node(child, gltf, buffer))
        .collect();
    if !children.is_empty() {
        entry["children"] = json!(children);
    }
    let nodes = gltf["nodes"].as_array_mut().unwrap();
    nodes.push(entry);
    nodes.len() - 1
}

fn add_accessor(values: &[Vec3], bounds: bool, gltf: &mut Value, buffer: &mut Vec<u8>) -> usize {
    let offset = buffer.len();
    for v in values {
        for f in v.to_array() {
            buffer.extend(f.to_le_bytes());
        }
    }
    let views = gltf["bufferViews"].as_array_mut().unwrap();
    views.push(json!({ "buffer": 0, "byteOffset": offset, "byteLength": buffer.len() - offset }));
    let mut accessor = json!({
        "bufferView": views.len() - 1,
        "componentType": 5126,
        "count": values.len(),
        "type": "VEC3",
    });
    if bounds {
        let min = values.iter().copied().reduce(Vec3::min).unwrap();
        let max = values.iter().copied().reduce(Vec3::max).unwrap();
        accessor["min"] = json!(min.to_array());
        accessor["max"] = json!(max.to_array());
    }
    let accessors = gltf["accessors"].as_array_mut().unwrap();
    accessors.push(accessor);
    accessors.len() - 1
}

fn main() {
    let mut gltf = json!({
        "asset": { "version": "2.0", "generator": "placefinder collision_test example" },
        "scene": 0,
        "nodes": [],
        "meshes": [],
        "materials": [{
            "name": "grey",
            "pbrMetallicRoughness": { "baseColorFactor": [0.6, 0.6, 0.62, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.9 },
        }],
        "accessors": [],
        "bufferViews": [],
    });
    let mut buffer = Vec::new();
    let roots: Vec<usize> = scene()
        .into_iter()
        .map(|node| write_node(node, &mut gltf, &mut buffer))
        .collect();
    gltf["scenes"] = json!([{ "name": "collision test", "nodes": roots }]);
    gltf["buffers"] = json!([{ "uri": "scene.bin", "byteLength": buffer.len() }]);

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../places/collision-test");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("scene.bin"), buffer).unwrap();
    let text = rmcp::serde_json::to_string_pretty(&gltf).unwrap();
    std::fs::write(dir.join("scene.gltf"), text + "\n").unwrap();
}
