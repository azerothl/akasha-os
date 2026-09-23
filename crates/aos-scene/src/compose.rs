//! Prompt → SceneGraph MVP composer (templates + keyword heuristics).
//!
//! No neural planner. SceneGraph remains the only SoT; this module only
//! instantiates pack prefabs and places a camera.

use crate::assets::{embedded_primitives_pack, instantiate_asset, AssetError, AssetPack};
use crate::math::{Quat, Vec3};
use crate::scene::{CameraParams, LightParams, NodeKind, SceneError, SceneGraph, SceneNode, Transform};
use thiserror::Error;

/// DeclUI / host service id: compose a SceneGraph from a short prompt.
pub const SCENE_COMPOSE_SERVICE: &str = "scene.compose";

/// Cap: run heuristic prompt → SceneGraph compose (fail-closed).
pub const SCENE_COMPOSE_CAP: &str = "scene.compose";

#[derive(Debug, Error, PartialEq)]
pub enum ComposeError {
    #[error("empty prompt")]
    EmptyPrompt,
    #[error("asset: {0}")]
    Asset(#[from] AssetError),
    #[error("scene: {0}")]
    Scene(String),
}

impl From<SceneError> for ComposeError {
    fn from(e: SceneError) -> Self {
        Self::Scene(e.to_string())
    }
}

/// Result of a compose pass.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposeResult {
    pub scene: SceneGraph,
    /// Human-readable template id chosen by heuristics (`interior_library`, `default_stage`, …).
    pub template_id: String,
    /// Asset ids instantiated (order of placement).
    pub placed_assets: Vec<String>,
    /// Root id of the primary character, if any.
    pub character_id: Option<String>,
    /// Active camera node id.
    pub camera_id: String,
}

/// Keyword-ish intent extracted from a free-form EN/FR prompt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposeIntent {
    pub wants_library: bool,
    pub wants_door: bool,
    pub wants_chair: bool,
    pub wants_counter: bool,
    pub wants_bookshelf: bool,
    pub wants_sofa: bool,
    pub wants_desk: bool,
    pub wants_table: bool,
    pub wants_wall: bool,
    pub wants_window: bool,
    pub wants_stairs: bool,
    pub wants_lamp: bool,
    pub wants_book: bool,
    pub wants_cup: bool,
    pub wants_plant: bool,
    pub wants_child: bool,
    pub wants_adult: bool,
    pub wants_female: bool,
    pub wants_second_character: bool,
    pub wants_cat: bool,
    pub wants_dog: bool,
}

impl ComposeIntent {
    pub fn from_prompt(prompt: &str) -> Self {
        let p = prompt.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| p.contains(w));
        let wants_library = has(&[
            "librar",
            "bookstore",
            "bookshop",
            "librairie",
            "bibliothèque",
            "bibliotheque",
            "bookshel",
            "étagère",
            "etagere",
        ]);
        let wants_door = has(&["door", "porte", "entrance", "entrée", "entree"]);
        let wants_chair = has(&["chair", "chaise", "seat", "siège", "siege"]);
        let wants_sofa = has(&["sofa", "couch", "canapé", "canape", "divan"]);
        let wants_desk = has(&["desk", "bureau", "writing desk"]);
        let wants_table = has(&["table"]) && !wants_desk;
        let wants_counter = has(&["counter", "comptoir"]) || wants_library;
        let wants_bookshelf = wants_library
            || has(&["shelf", "shelves", "bookshelf", "rayonnage", "étagère", "etagere"]);
        let wants_wall = wants_library || has(&["wall", "mur", "walls", "murs"]);
        let wants_window =
            wants_library || has(&["window", "fenêtre", "fenetre", "vitrine"]);
        let wants_stairs = has(&["stair", "stairs", "escalier", "steps", "marches"]);
        let wants_lamp =
            wants_library || has(&["lamp", "lampe", "lantern", "lanterne", "lumière", "lumiere"]);
        let wants_book = wants_library || has(&["book", "livre", "tome", "novel", "roman"]);
        let wants_cup = has(&["cup", "mug", "tasse", "coffee", "café", "cafe", "tea", "thé"]);
        let wants_plant =
            wants_library || has(&["plant", "plante", "flower", "fleur", "fern", "fougère", "fougere"]);
        let wants_child = has(&["child", "kid", "enfant", "fille", "garçon", "garcon"]);
        let wants_female = has(&[
            "woman",
            "female",
            "femme",
            "lady",
            "dame",
            "girl",
            "fille",
        ]);
        let wants_adult = has(&[
            "man",
            "woman",
            "homme",
            "femme",
            "person",
            "personne",
            "human",
            "humanoïde",
            "humanoide",
            "character",
            "personnage",
            "adult",
            "adulte",
        ]);
        let wants_second_character = has(&["two", "deux", "both", "pair", "together", "ensemble"])
            || (wants_child && wants_adult);
        let wants_cat = has(&[
            "cat",
            "chat",
            "kitten",
            "chaton",
            "félin",
            "felin",
            "quadruped",
            "animal",
        ]) && !has(&["dog", "chien", "puppy", "chiot", "hound"]);
        let wants_dog = has(&["dog", "chien", "puppy", "chiot", "hound"]);
        Self {
            wants_library,
            wants_door,
            wants_chair,
            wants_counter,
            wants_bookshelf,
            wants_sofa,
            wants_desk,
            wants_table,
            wants_wall,
            wants_window,
            wants_stairs,
            wants_lamp,
            wants_book,
            wants_cup,
            wants_plant,
            wants_child,
            wants_adult,
            wants_female,
            wants_second_character,
            wants_cat,
            wants_dog,
        }
    }
}

/// Compose a SceneGraph from a short prompt using pack prefabs + heuristics.
pub fn compose_from_prompt(prompt: &str) -> Result<ComposeResult, ComposeError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(ComposeError::EmptyPrompt);
    }
    let pack = embedded_primitives_pack()?;
    compose_from_prompt_with_pack(prompt, &pack)
}

pub fn compose_from_prompt_with_pack(
    prompt: &str,
    pack: &AssetPack,
) -> Result<ComposeResult, ComposeError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(ComposeError::EmptyPrompt);
    }
    let intent = ComposeIntent::from_prompt(prompt);
    let template_id = if intent.wants_library {
        "interior_library"
    } else if intent.wants_counter
        || intent.wants_bookshelf
        || intent.wants_door
        || intent.wants_chair
        || intent.wants_sofa
        || intent.wants_desk
        || intent.wants_table
        || intent.wants_wall
        || intent.wants_window
        || intent.wants_stairs
    {
        "interior_room"
    } else {
        "default_stage"
    };

    let mut scene = empty_rooted_scene();
    let mut placed = Vec::new();

    // Ground always.
    place(
        &mut scene,
        pack,
        "prop.ground",
        "ground_",
        Vec3::new(0.0, 0.04, 0.0),
        &mut placed,
    )?;

    if template_id == "interior_library" || template_id == "interior_room" {
        if intent.wants_wall {
            place(
                &mut scene,
                pack,
                "arch.wall",
                "wall_",
                Vec3::new(0.0, 0.0, -2.4),
                &mut placed,
            )?;
            let side_wall = place(
                &mut scene,
                pack,
                "arch.wall",
                "wallb_",
                Vec3::new(-3.2, 0.0, 0.0),
                &mut placed,
            )?;
            if let Some(node) = scene.nodes.get_mut(&side_wall) {
                // Rotate side wall ~90° around Y (xyzw).
                node.transform.rotation =
                    Quat::from_axis_angle(Vec3::UNIT_Y, std::f32::consts::FRAC_PI_2);
            }
        }
        if intent.wants_window {
            place(
                &mut scene,
                pack,
                "arch.window",
                "window_",
                Vec3::new(1.4, 0.0, -2.25),
                &mut placed,
            )?;
        }
        if intent.wants_bookshelf || intent.wants_library {
            place(
                &mut scene,
                pack,
                "prop.bookshelf",
                "shelf_",
                Vec3::new(-2.2, 0.0, -1.5),
                &mut placed,
            )?;
            place(
                &mut scene,
                pack,
                "prop.bookshelf",
                "shelfb_",
                Vec3::new(2.2, 0.0, -1.5),
                &mut placed,
            )?;
        }
        if intent.wants_counter || intent.wants_library {
            place(
                &mut scene,
                pack,
                "prop.counter",
                "counter_",
                Vec3::new(0.0, 0.0, -0.4),
                &mut placed,
            )?;
        }
        if intent.wants_door || intent.wants_library {
            place(
                &mut scene,
                pack,
                "prop.door",
                "door_",
                Vec3::new(-2.8, 0.0, 1.8),
                &mut placed,
            )?;
        }
        if intent.wants_chair {
            place(
                &mut scene,
                pack,
                "prop.chair",
                "chair_",
                Vec3::new(1.2, 0.0, 0.8),
                &mut placed,
            )?;
        }
        if intent.wants_sofa || intent.wants_library {
            place(
                &mut scene,
                pack,
                "prop.sofa",
                "sofa_",
                Vec3::new(2.0, 0.0, 1.2),
                &mut placed,
            )?;
        }
        if intent.wants_desk {
            place(
                &mut scene,
                pack,
                "prop.desk",
                "desk_",
                Vec3::new(-1.4, 0.0, 0.6),
                &mut placed,
            )?;
        }
        if intent.wants_table {
            place(
                &mut scene,
                pack,
                "prop.table",
                "table_",
                Vec3::new(0.4, 0.0, 1.6),
                &mut placed,
            )?;
        }
        if intent.wants_stairs {
            place(
                &mut scene,
                pack,
                "arch.stairs",
                "stairs_",
                Vec3::new(2.6, 0.0, -0.8),
                &mut placed,
            )?;
        }
        if intent.wants_lamp {
            place(
                &mut scene,
                pack,
                "prop.lamp",
                "lamp_",
                Vec3::new(0.7, 0.9, -0.2),
                &mut placed,
            )?;
        }
        if intent.wants_book {
            place(
                &mut scene,
                pack,
                "prop.book",
                "book_",
                Vec3::new(-0.4, 0.92, -0.15),
                &mut placed,
            )?;
            place(
                &mut scene,
                pack,
                "prop.book",
                "bookb_",
                Vec3::new(-0.55, 0.96, -0.1),
                &mut placed,
            )?;
        }
        if intent.wants_cup {
            place(
                &mut scene,
                pack,
                "prop.cup",
                "cup_",
                Vec3::new(0.35, 0.95, -0.25),
                &mut placed,
            )?;
        }
        if intent.wants_plant {
            place(
                &mut scene,
                pack,
                "prop.plant",
                "plant_",
                Vec3::new(-2.4, 0.0, 1.4),
                &mut placed,
            )?;
        }
    } else {
        // Stage blockout: pedestal + box so beauty/viewport stay interesting.
        place(
            &mut scene,
            pack,
            "prop.pedestal",
            "ped_",
            Vec3::new(0.0, 0.175, 0.0),
            &mut placed,
        )?;
        place(
            &mut scene,
            pack,
            "prop.box",
            "box_",
            Vec3::new(0.0, 0.675, 0.0),
            &mut placed,
        )?;
        if intent.wants_lamp {
            place(
                &mut scene,
                pack,
                "prop.lamp",
                "lamp_",
                Vec3::new(-1.0, 0.0, 0.4),
                &mut placed,
            )?;
        }
        if intent.wants_plant {
            place(
                &mut scene,
                pack,
                "prop.plant",
                "plant_",
                Vec3::new(1.2, 0.0, -0.6),
                &mut placed,
            )?;
        }
    }

    let mut character_id = None;
    if intent.wants_adult || intent.wants_child {
        let primary_asset = if intent.wants_child && !intent.wants_adult {
            "humanoid.slim"
        } else if intent.wants_female {
            "humanoid.female"
        } else {
            "humanoid.placeholder"
        };
        let primary_pos = if template_id.starts_with("interior") {
            Vec3::new(0.8, 0.0, 1.4)
        } else {
            Vec3::new(1.4, 0.0, 0.3)
        };
        let root = place(
            &mut scene,
            pack,
            primary_asset,
            "char_",
            primary_pos,
            &mut placed,
        )?;
        character_id = Some(root);

        if intent.wants_second_character {
            let secondary = if primary_asset == "humanoid.placeholder"
                || primary_asset == "humanoid.female"
            {
                "humanoid.slim"
            } else if intent.wants_female {
                "humanoid.female"
            } else {
                "humanoid.placeholder"
            };
            let _ = place(
                &mut scene,
                pack,
                secondary,
                "char2_",
                Vec3::new(-0.6, 0.0, 1.2),
                &mut placed,
            )?;
        }
    }

    if intent.wants_cat {
        let cat_pos = if template_id.starts_with("interior") {
            Vec3::new(-0.2, 0.9, 0.4)
        } else {
            Vec3::new(-0.8, 0.0, 0.6)
        };
        let cat_root = place(
            &mut scene,
            pack,
            "quadruped.cat",
            "cat_",
            cat_pos,
            &mut placed,
        )?;
        if character_id.is_none() {
            character_id = Some(cat_root);
        }
    }

    if intent.wants_dog {
        let dog_pos = if template_id.starts_with("interior") {
            Vec3::new(-1.0, 0.0, 1.0)
        } else {
            Vec3::new(-1.2, 0.0, 0.8)
        };
        let dog_root = place(
            &mut scene,
            pack,
            "quadruped.dog",
            "dog_",
            dog_pos,
            &mut placed,
        )?;
        if character_id.is_none() {
            character_id = Some(dog_root);
        }
    }

    let camera_id = add_camera(
        &mut scene,
        if template_id.starts_with("interior") {
            Vec3::new(0.0, 2.0, 5.5)
        } else {
            Vec3::new(0.0, 2.2, 6.5)
        },
    )?;
    let _ = add_key_light(&mut scene)?;

    scene.validate()?;
    Ok(ComposeResult {
        scene,
        template_id: template_id.into(),
        placed_assets: placed,
        character_id,
        camera_id,
    })
}

pub(crate) fn empty_rooted_scene() -> SceneGraph {
    let mut nodes = std::collections::HashMap::new();
    let root = SceneNode::empty("root", "Scene");
    nodes.insert(root.id.clone(), root);
    SceneGraph {
        nodes,
        roots: vec!["root".into()],
        active_camera: None,
    }
}

fn place(
    scene: &mut SceneGraph,
    pack: &AssetPack,
    asset_id: &str,
    prefix: &str,
    translation: Vec3,
    placed: &mut Vec<String>,
) -> Result<String, ComposeError> {
    let inst = instantiate_asset(scene, pack, asset_id, Some("root"), prefix)?;
    if let Some(node) = scene.nodes.get_mut(&inst.root_id) {
        node.transform.translation = translation;
    }
    placed.push(asset_id.into());
    Ok(inst.root_id)
}

pub(crate) fn add_camera(scene: &mut SceneGraph, translation: Vec3) -> Result<String, ComposeError> {
    let id = "camera".to_string();
    if scene.nodes.contains_key(&id) {
        return Ok(id);
    }
    let mut cam = SceneNode::empty(&id, "Camera");
    cam.kind = NodeKind::Camera;
    cam.parent = Some("root".into());
    cam.transform = Transform {
        translation,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };
    cam.camera = Some(CameraParams::default());
    scene.nodes.insert(id.clone(), cam);
    if let Some(root) = scene.nodes.get_mut("root") {
        if !root.children.contains(&id) {
            root.children.push(id.clone());
        }
    }
    scene.active_camera = Some(id.clone());
    Ok(id)
}

pub(crate) fn add_key_light(scene: &mut SceneGraph) -> Result<String, ComposeError> {
    let id = "key_light".to_string();
    if scene.nodes.contains_key(&id) {
        return Ok(id);
    }
    scene
        .insert_light(
            &id,
            "Key Light",
            Some("root"),
            Transform {
                translation: Vec3::new(2.8, 4.5, 3.2),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            LightParams::default(),
        )
        .map_err(ComposeError::from)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_prompt_places_shelves_counter_humanoid_camera() {
        let r = compose_from_prompt(
            "Un homme entre dans une vieille librairie. Un chat est couché sur le comptoir.",
        )
        .expect("compose");
        assert_eq!(r.template_id, "interior_library");
        assert!(r.placed_assets.iter().any(|a| a == "prop.bookshelf"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.counter"));
        assert!(r.placed_assets.iter().any(|a| a == "arch.wall"));
        assert!(r.placed_assets.iter().any(|a| a == "arch.window"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.sofa"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.lamp"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.book"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.plant"));
        assert!(r.placed_assets.iter().any(|a| a == "humanoid.placeholder"));
        assert!(r.placed_assets.iter().any(|a| a == "quadruped.cat"));
        assert!(r.scene.nodes.contains_key("camera"));
        assert_eq!(r.scene.active_camera.as_deref(), Some("camera"));
        r.scene.validate().expect("valid");
    }

    #[test]
    fn child_prompt_uses_slim_variant() {
        let r = compose_from_prompt("a child stands near a chair").expect("compose");
        assert!(r.placed_assets.iter().any(|a| a == "humanoid.slim"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.chair"));
    }

    #[test]
    fn woman_prompt_uses_female_variant() {
        let r = compose_from_prompt("a woman sits on a sofa near a desk").expect("compose");
        assert!(r.placed_assets.iter().any(|a| a == "humanoid.female"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.sofa"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.desk"));
    }

    #[test]
    fn dog_prompt_places_quadruped_without_forced_humanoid() {
        let r = compose_from_prompt("a dog near a plant").expect("compose");
        assert!(r.placed_assets.iter().any(|a| a == "quadruped.dog"));
        assert!(r.placed_assets.iter().any(|a| a == "prop.plant"));
        assert!(!r.placed_assets.iter().any(|a| a.starts_with("humanoid.")));
    }

    #[test]
    fn empty_prompt_fails() {
        assert!(matches!(
            compose_from_prompt("   "),
            Err(ComposeError::EmptyPrompt)
        ));
    }

    #[test]
    fn default_stage_has_pedestal_and_character() {
        let r = compose_from_prompt("a person on a stage").expect("compose");
        assert_eq!(r.template_id, "default_stage");
        assert!(r.character_id.is_some());
        assert!(r.placed_assets.iter().any(|a| a == "prop.pedestal"));
    }

    #[test]
    fn object_only_prompt_does_not_add_a_humanoid() {
        let intent = ComposeIntent::from_prompt("A quiet room with a table and a window");
        assert!(!intent.wants_adult);
        assert!(!intent.wants_child);
    }

    #[test]
    fn bookstore_demo_prompt_is_coherent() {
        let r = compose_from_prompt(
            "An old bookstore. A man enters through the door while a cat lies on the counter watching him.",
        )
        .expect("compose");
        assert_eq!(r.template_id, "interior_library");
        for id in [
            "prop.door",
            "prop.counter",
            "prop.bookshelf",
            "arch.wall",
            "arch.window",
            "prop.lamp",
            "prop.book",
            "humanoid.placeholder",
            "quadruped.cat",
        ] {
            assert!(
                r.placed_assets.iter().any(|a| a == id),
                "missing asset {id} in {:?}",
                r.placed_assets
            );
        }
        r.scene.validate().expect("valid");
    }
}
