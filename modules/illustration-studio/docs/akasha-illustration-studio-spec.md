# Akasha Illustration Studio

**Status:** Draft  
**Target:** Akasha OS  
**Module:** `illustration-studio`  
**Runtime extension:** `scene3d`  
**Version:** 0.1

---

# 1. Vision

`Illustration Studio` est un module Akasha OS permettant de créer une illustration à partir d'une **composition 3D entièrement éditable**.

L'objectif n'est pas de générer directement une image depuis un prompt.

Le workflow est :

```text
Prompt
  ↓
Scene understanding
  ↓
3D scene generation
  ↓
Editable composition
  ↓
Pose / camera / lighting editing
  ↓
Illustration rendering
```

Exemple :

> Un homme entre dans une vieille librairie. Un chat est couché sur le comptoir et regarde l'homme.

Le système produit une scène contenant :

```text
OldBookstore
├── Architecture
├── Bookshelves
├── Books
├── Counter
│   └── Cat
│       └── AnimalRig
├── Door
├── Man
│   └── HumanoidRig
├── Lights
└── Camera
```

L'utilisateur peut ensuite modifier :

- position des objets ;
- orientation ;
- taille ;
- caméra ;
- focale ;
- éclairage ;
- pose des personnages ;
- pose des animaux ;
- expression ;
- direction du regard ;
- cadrage ;
- style d'illustration.

Le rendu final peut utiliser différents styles :

- esquisse ;
- crayon ;
- fusain ;
- encre ;
- feutre ;
- bande dessinée ;
- aquarelle ;
- peinture ;
- illustration jeunesse ;
- etc.

La **scène 3D reste toujours la source de vérité**.

---

# 2. Principes fondamentaux

## 2.1 3D first

L'image finale n'est jamais la donnée principale.

```text
Scene3D → Renderer → Image
```

et non :

```text
Prompt → Image
```

Cela permet de modifier la scène après génération.

---

## 2.2 Non-destructive workflow

Toutes les opérations sont non destructives.

Exemple :

```text
Scene v1
  ↓
change pose
  ↓
Scene v2
  ↓
change camera
  ↓
Scene v3
```

Les paramètres de rendu sont séparés de la scène.

```text
Scene
+
RenderPreset
=
Illustration
```

---

## 2.3 Human + Agent co-editing

L'utilisateur et l'agent manipulent **le même SceneGraph**.

```text
              ┌──────────┐
              │  Agent   │
              └────┬─────┘
                   │
                   ▼
User ────────► SceneGraph ◄──────── UI
                   │
                   ▼
                Renderer
```

L'agent ne génère donc pas une représentation parallèle cachée.

---

# 3. Architecture générale

```text
┌───────────────────────────────────────────────┐
│                 Akasha OS                     │
│                                               │
│  ┌─────────────────────────────────────────┐  │
│  │          Illustration Studio            │  │
│  │                                         │  │
│  │ Prompt                                  │  │
│  │   ↓                                     │  │
│  │ Scene Planner                           │  │
│  │   ↓                                     │  │
│  │ Asset Resolver                          │  │
│  │   ↓                                     │  │
│  │ Scene Composer                          │  │
│  │   ↓                                     │  │
│  │ SceneGraph                              │  │
│  └───────────────────┬─────────────────────┘  │
│                      │                        │
│              declarative_ui                  │
│                      │                        │
│               ┌──────▼──────┐                │
│               │   scene3d   │                │
│               └──────┬──────┘                │
│                      │                        │
│                 GPU service                  │
│                      │                        │
│               Render backend                 │
│                      │                        │
│              NPR illustration                │
└───────────────────────────────────────────────┘
```

---

# 4. Séparation des responsabilités

Deux projets doivent être distingués.

## Akasha Runtime

Le runtime fournit :

```text
scene3d widget
SceneGraph primitives
3D interaction
GPU access
asset loading
capabilities
render service API
```

Il ne connaît pas la notion métier d'illustration.

---

## Illustration Studio

Le module fournit :

```text
prompt interpretation
scene planning
asset selection
character posing
composition
illustration styles
render orchestration
project management
```

Cette séparation permettra à d'autres modules Akasha d'utiliser `scene3d`.

Exemples :

```text
architecture-studio
game-level-editor
robotics-simulator
cad-viewer
scientific-visualizer
virtual-world-editor
```

---

# 5. Nouveau widget Akasha : `scene3d`

Akasha dispose actuellement d'une UI déclarative fermée.

Il faut ajouter une primitive :

```yaml
type: scene3d
```

Elle ne doit pas exécuter du JavaScript arbitraire.

---

# 6. Exemple de déclaration

```yaml
type: scene3d

source:
  scene: project.scene

camera:
  active: main-camera
  editable: true

viewport:
  orbit: true
  pan: true
  zoom: true
  grid: true

selection:
  enabled: true
  multi_select: true

transform:
  translate: true
  rotate: true
  scale: true

rig:
  enabled: true
  show_skeleton: on_select
  joint_selection: true
  ik: true
  fk: true

lighting:
  editable: true

render:
  preview: true
```

---

# 7. SceneGraph

Le cœur du système est un format de scène indépendant du moteur de rendu.

Exemple :

```yaml
scene:
  id: bookstore_scene

  units: meters

  environment:
    type: interior

  nodes:

    - id: bookstore
      type: group

    - id: counter
      type: mesh
      asset: asset://furniture/wood_counter
      parent: bookstore

      transform:
        position: [0, 0, 0]
        rotation: [0, 0, 0]
        scale: [1, 1, 1]

    - id: man
      type: character
      asset: asset://characters/human/male_generic

      transform:
        position: [-2.4, 0, 1.1]

      rig:
        type: humanoid

    - id: cat
      type: character
      asset: asset://animals/cat

      parent: counter

      rig:
        type: quadruped

    - id: camera
      type: camera

      camera:
        focal_length: 45
        sensor: 36

    - id: key_light
      type: light

      light:
        type: area
        intensity: 800
```

---

# 8. Types de nodes

Le MVP doit supporter :

```text
group
mesh
character
camera
light
empty
```

Puis :

```text
curve
volume
particles
terrain
text
procedural
```

---

# 9. Transform

Tous les objets possèdent :

```yaml
transform:

  position:
    x: 0
    y: 0
    z: 0

  rotation:
    x: 0
    y: 0
    z: 0

  scale:
    x: 1
    y: 1
    z: 1
```

Le runtime doit exposer les transformations au widget et aux agents.

---

# 10. Prompt → Scene

Le Scene Planner reçoit :

```text
Un homme entre dans une vieille librairie.
Un chat est couché sur le comptoir.
```

Il doit produire une représentation intermédiaire.

```json
{
  "environment": {
    "type": "bookstore",
    "style": "old"
  },

  "subjects": [
    {
      "type": "human",
      "gender": "male",
      "action": "entering"
    },

    {
      "type": "cat",
      "action": "lying",
      "location": "counter"
    }
  ],

  "objects": [
    "counter",
    "bookshelves",
    "books",
    "door"
  ]
}
```

Cette représentation est appelée :

```text
SceneIntent
```

---

# 11. SceneIntent

Le `SceneIntent` décrit la **sémantique**, pas la géométrie.

```text
Prompt
   ↓
SceneIntent
   ↓
SceneGraph
```

Cela permet de changer de stratégie de génération sans modifier l'UI.

---

# 12. Scene Planner

Responsabilités :

1. identifier l'environnement ;
2. identifier les personnages ;
3. identifier les objets ;
4. comprendre les actions ;
5. comprendre les relations spatiales ;
6. déterminer les éléments importants ;
7. proposer une composition.

Exemple :

```text
cat ON counter

man NEAR door

man FACING counter

cat LOOKING_AT man
```

---

# 13. Spatial constraints

Le SceneIntent doit supporter :

```text
ON
UNDER
ABOVE
INSIDE
NEXT_TO
BEHIND
IN_FRONT_OF
NEAR
FAR
FACING
LOOKING_AT
TOUCHING
HOLDING
```

Exemple :

```yaml
constraints:

  - relation: ON
    subject: cat
    object: counter

  - relation: LOOKING_AT
    subject: cat
    object: man

  - relation: NEAR
    subject: man
    object: door
```

---

# 14. Asset Resolver

Le Scene Planner ne choisit pas directement les meshes.

Il demande :

```text
asset.find("old wooden bookstore counter")
```

L'Asset Resolver cherche :

1. bibliothèque locale ;
2. assets installés ;
3. assets communautaires ;
4. éventuellement source distante autorisée ;
5. génération procédurale ;
6. génération 3D IA ultérieure.

---

# 15. Asset Library

Organisation possible :

```text
assets/

  architecture/
  furniture/
  props/

  characters/
    human/
    animals/

  vegetation/

  lights/

  materials/

  environments/
```

Chaque asset possède des métadonnées.

```yaml
id: furniture.counter.old_wood

tags:
  - counter
  - wooden
  - antique
  - shop

dimensions:
  width: 2.1
  height: 1.05
  depth: 0.65

license: CC0

rigged: false
```

---

# 16. Recherche sémantique

La recherche d'assets doit utiliser :

```text
text embedding
+
tags
+
metadata
```

Exemple :

```text
"old bookstore furniture"
```

peut retourner :

```text
Victorian wooden counter
Antique bookshelf
Wooden ladder
Old reading table
```

---

# 17. Personnages

Les personnages utilisent des rigs normalisés.

Pour les humains :

```text
HumanoidRig v1
```

Structure simplifiée :

```text
root
└── pelvis
    ├── spine
    │   ├── chest
    │   │   ├── neck
    │   │   │   └── head
    │   │   ├── arm.L
    │   │   └── arm.R
    ├── leg.L
    └── leg.R
```

---

# 18. Animaux

Prévoir plusieurs profils.

```text
quadruped
bird
reptile
custom
```

Exemple :

```yaml
rig:
  profile: quadruped.cat
```

---

# 19. Manipulation des poses

L'utilisateur peut sélectionner un personnage.

Le widget affiche alors le squelette.

```text
      Head ●
           │
      Neck ●
           │
           ● Chest
          / \
         /   \
        ●     ●
       arm   arm
```

Les articulations deviennent manipulables.

---

# 20. IK / FK

Les deux modes doivent être disponibles.

### FK

L'utilisateur tourne directement les articulations.

### IK

L'utilisateur déplace :

```text
hand
foot
head target
hip
```

et le solveur calcule la chaîne.

Exemple :

```text
drag hand
   ↓
IK solver
   ↓
shoulder rotation
elbow rotation
wrist rotation
```

---

# 21. Pose controls

Des contrôleurs simplifiés doivent exister :

```text
Head
Look target

Left hand
Right hand

Pelvis

Left foot
Right foot
```

Cela évite d'imposer la manipulation de dizaines d'os.

---

# 22. Pose presets

La bibliothèque peut fournir :

```text
standing
walking
running
sitting
lying
reading
talking
pointing
holding
```

L'agent peut appliquer :

```text
rig.apply_pose(man, "entering-door")
```

puis ajuster la pose.

---

# 23. Pose générée par langage naturel

L'utilisateur peut également demander :

> Fais en sorte que l'homme regarde le chat tout en tenant la porte ouverte.

Le système produit :

```text
pose.plan
```

puis :

```text
rig.look_at
rig.hand_to
rig.solve_ik
```

---

# 24. Expressions

Une extension ultérieure pourra gérer :

```text
face rig
blend shapes
expressions
```

Presets :

```text
neutral
happy
angry
sad
surprised
afraid
```

---

# 25. Composition

Le module doit comprendre les principes de composition.

Presets :

```text
rule_of_thirds
centered
symmetrical
golden_ratio
close_up
medium_shot
wide_shot
over_shoulder
low_angle
high_angle
```

---

# 26. Camera Agent

Un sous-système peut proposer :

```text
composition.suggest()
```

Exemple :

```text
Camera:
35 mm

Position:
behind / right of man

Focus:
cat

Secondary subject:
man

Composition:
rule of thirds
```

---

# 27. Modification caméra

L'utilisateur peut modifier :

```text
position
rotation
focal length
depth of field
focus target
sensor
aspect ratio
```

---

# 28. Lighting

MVP :

```text
directional
point
spot
area
environment
```

Presets :

```text
soft daylight
dramatic
warm interior
overcast
night
studio
```

---

# 29. Pipeline de rendu

Architecture :

```text
SceneGraph
    │
    ▼
Scene Adapter
    │
    ▼
Render Backend
    │
    ├── Geometry pass
    ├── Depth pass
    ├── Normal pass
    ├── Object ID
    ├── Material ID
    ├── Line pass
    └── Shadow pass
          │
          ▼
     NPR Pipeline
          │
          ▼
      Illustration
```

---

# 30. Render backend initial

Pour le MVP :

**Blender headless** est un très bon candidat.

Akasha génère :

```text
SceneGraph
```

Un adapter transforme la scène en représentation Blender.

```text
SceneGraph
    ↓
Blender Adapter
    ↓
Blender Scene
```

Puis :

```text
Blender
+
Freestyle / Line Art
+
Shaders
+
Compositor
```

---

# 31. Important : Blender n'est pas le format projet

Ne jamais stocker uniquement :

```text
project.blend
```

Le format canonique reste :

```text
scene.yaml
```

ou :

```text
scene.json
```

Blender est interchangeable.

À terme :

```text
SceneGraph
 ├── Blender backend
 ├── Native GPU backend
 ├── Godot backend
 └── Remote renderer
```

---

# 32. NPR Rendering

NPR signifie :

**Non-Photorealistic Rendering**

Il permet d'obtenir une apparence dessinée à partir de la géométrie.

---

# 33. Edge extraction

Sources :

```text
silhouette
crease
material boundary
depth discontinuity
normal discontinuity
shadow boundary
```

Ces informations génèrent le line art.

---

# 34. Style : Pencil

Pipeline :

```text
geometry
 ↓
line extraction
 ↓
line jitter
 ↓
pressure simulation
 ↓
graphite texture
 ↓
cross-hatching
 ↓
paper texture
```

Paramètres :

```yaml
pencil:

  line_width: 1.2

  jitter: 0.15

  graphite:
    hardness: HB

  shading:
    mode: cross_hatching

  paper:
    texture: rough
```

---

# 35. Style : Ink

```text
geometry
 ↓
line extraction
 ↓
variable line width
 ↓
black ink
 ↓
minimal shading
```

Paramètres :

```yaml
ink:

  pressure_variation: 0.4

  line_width:
    min: 0.5
    max: 3

  shading:
    mode: hatch
```

---

# 36. Style : Marker

```text
Line Art
+
flat colors
+
color bleeding
+
paper texture
```

---

# 37. Style : Watercolor

Pipeline plus complexe :

```text
base color
 ↓
color quantization
 ↓
water diffusion
 ↓
edge pooling
 ↓
pigment variation
 ↓
paper absorption
 ↓
paper texture
```

L'aquarelle devra probablement arriver après le MVP.

---

# 38. Style definition

Les styles doivent être des données.

```yaml
style:
  id: pencil_classic

  renderer: npr

  line:
    type: pencil

  shading:
    type: cross_hatching

  color:
    saturation: 0

  texture:
    paper: rough_01
```

Ainsi la communauté pourra créer des styles sans modifier le moteur.


---

# 39. Style inheritance

Les styles doivent pouvoir hériter d'autres styles.

```yaml
id: pencil_soft
extends: pencil_classic

line:
  jitter: 0.25
  opacity: 0.75

shading:
  contrast: 0.8

paper:
  texture: watercolor_paper_01
```

Cela permet de créer facilement des variantes.

```text
pencil
├── pencil_classic
├── pencil_soft
├── pencil_dark
└── pencil_blue

ink
├── ink_clean
├── ink_comic
└── ink_brush

watercolor
├── watercolor_soft
├── watercolor_storybook
└── watercolor_wet
```

---

# 40. Style Packs

Les styles peuvent être distribués sous forme de packs.

```text
styles/
└── traditional-drawing/
    ├── manifest.yaml
    ├── pencil.yaml
    ├── charcoal.yaml
    ├── ink.yaml
    ├── marker.yaml
    └── watercolor.yaml
```

Exemple :

```yaml
id: traditional-drawing

name: Traditional Drawing

version: 1.0.0

styles:
  - pencil
  - charcoal
  - ink
  - marker
  - watercolor
```

Ces packs pourraient à terme utiliser le mécanisme de distribution de modules/assets d'Akasha.

---

# 41. Render presets vs styles

Il faut distinguer :

```text
Style
```

et :

```text
RenderPreset
```

Le style décrit l'apparence.

Le preset décrit les paramètres de production.

Exemple :

```yaml
preset:
  id: preview-pencil

  style: pencil_classic

  resolution:
    width: 1024
    height: 1024

  samples: 32

  quality: preview
```

contre :

```yaml
preset:
  id: final-pencil

  style: pencil_classic

  resolution:
    width: 4096
    height: 4096

  samples: 256

  quality: final
```

---

# 42. Preview Rendering

Le workflow doit favoriser des previews rapides.

Lorsque l'utilisateur manipule la scène :

```text
3D viewport
```

reste temps réel.

Lorsqu'il demande :

> Preview illustration

le système produit un rendu basse résolution.

Exemple :

```text
512 × 512
```

ou :

```text
1024 × 1024
```

Le rendu final n'est lancé qu'à la demande.

---

# 43. Progressive rendering

Une évolution intéressante consiste à rendre progressivement.

```text
128 px
 ↓
256 px
 ↓
512 px
 ↓
1024 px
```

L'utilisateur voit rapidement si :

- le cadrage fonctionne ;
- la pose fonctionne ;
- les silhouettes sont lisibles ;
- le style convient.

Il peut interrompre le rendu à tout moment.

---

# 44. Render regions

L'utilisateur doit pouvoir sélectionner une zone.

```text
┌───────────────────────────────┐
│                               │
│       ┌─────────────┐         │
│       │    CAT      │         │
│       │             │         │
│       └─────────────┘         │
│                               │
└───────────────────────────────┘
```

Puis :

```text
Render selection
```

Cela permet de tester rapidement un visage, une main, un animal ou une texture.

---

# 45. Application UI

Interface proposée :

```text
┌─────────────────────────────────────────────────────────────────┐
│ Illustration Studio                              Render ▸        │
├──────────────┬───────────────────────────────┬──────────────────┤
│              │                               │                  │
│ SCENE        │                               │ PROPERTIES       │
│              │                               │                  │
│ ▾ Bookstore  │                               │ Man              │
│   Shelves    │                               │                  │
│   Counter    │         3D VIEWPORT           │ Transform        │
│   Cat        │                               │ Position         │
│   Man        │                               │ Rotation         │
│   Camera     │                               │                  │
│   Lights     │                               │ Rig              │
│              │                               │ Pose             │
│              │                               │                  │
├──────────────┴───────────────────────────────┴──────────────────┤
│ Ask Akasha...                                                   │
│                                                                │
│ "Make the man look at the cat"                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

# 46. Workspace modes

L'interface peut proposer plusieurs modes.

```text
Compose
Pose
Camera
Lighting
Style
Render
```

Cela évite une interface 3D trop complexe.

---

# 47. Compose Mode

Permet de :

```text
select
move
rotate
scale
duplicate
delete
group
```

les éléments.

L'agent peut effectuer exactement les mêmes opérations.

---

# 48. Pose Mode

Lorsqu'un personnage est sélectionné :

```text
Pose Mode
```

affiche :

- skeleton ;
- IK targets ;
- rotation handles ;
- pose presets ;
- expression controls.

Les meshes de l'environnement deviennent moins visuellement présents pour faciliter le posing.

---

# 49. Camera Mode

Affiche :

```text
Camera
Lens
Composition
Depth of field
Focus
Aspect ratio
```

et éventuellement plusieurs caméras.

```text
Camera 1
Camera 2
Camera 3
```

L'utilisateur peut comparer plusieurs compositions sans modifier la scène.

---

# 50. Lighting Mode

Affiche les lumières directement dans la scène.

L'utilisateur peut déplacer :

```text
key light
fill light
rim light
```

ou demander :

> Donne une ambiance chaude de fin d'après-midi à la librairie.

L'agent modifie alors uniquement les lumières et éventuellement l'environnement.

---

# 51. Style Mode

Affichage proposé :

```text
STYLE

○ Sketch
○ Pencil
○ Charcoal
○ Ink
○ Marker
○ Watercolor
○ Comic

Variation

[────────●────]

Line strength

[────●────────]

Paper

[ Rough Paper ▼ ]
```

Le viewport peut proposer une preview NPR temps réel approximative.

---

# 52. Render Mode

Permet de définir :

```text
resolution
aspect ratio
style
quality
transparent background
output format
```

Formats initiaux :

```text
PNG
JPEG
WebP
```

Puis :

```text
SVG
PDF
PSD
OpenEXR
```

selon les capacités du backend.

---

# 53. Agent interaction

L'une des forces principales du module doit être la possibilité de modifier la scène en langage naturel.

Exemples :

> Mets le chat plus près du bord du comptoir.

> Fais regarder l'homme vers le chat.

> La pose de l'homme est trop rigide.

> Passe sur une focale 35 mm.

> Mets la caméra légèrement plus basse.

> Fais entrer davantage de lumière par la fenêtre.

> Je veux une composition plus dramatique.

> Transforme le rendu en esquisse au crayon.

---

# 54. Agent Scene Tools

Le module expose des tools structurés.

## Scene

```text
scene.get
scene.inspect
scene.create
scene.reset
```

## Nodes

```text
scene.node.add
scene.node.remove
scene.node.clone
scene.node.get
scene.node.update
```

## Transform

```text
scene.transform.translate
scene.transform.rotate
scene.transform.scale
scene.transform.set
```

## Assets

```text
asset.search
asset.inspect
asset.load
asset.replace
```

## Rig

```text
rig.inspect
rig.get_pose
rig.set_joint
rig.apply_pose
rig.solve_ik
rig.look_at
rig.reset_pose
```

## Camera

```text
camera.create
camera.set_active
camera.transform
camera.set_lens
camera.focus
```

## Lighting

```text
light.add
light.remove
light.update
light.apply_preset
```

## Composition

```text
composition.analyze
composition.suggest
composition.apply
```

## Render

```text
render.preview
render.region
render.final
render.cancel
```

---

# 55. Semantic scene inspection

Il ne faut surtout pas obliger l'agent à interpréter des pixels du viewport pour comprendre la scène.

Il doit pouvoir demander :

```text
scene.inspect()
```

Réponse :

```json
{
  "subjects": [
    {
      "id": "man",
      "type": "human",
      "position": [-2.4, 0.2, 0],
      "pose": "walking",
      "lookingAt": "cat"
    },
    {
      "id": "cat",
      "type": "cat",
      "parent": "counter",
      "pose": "lying"
    }
  ]
}
```

L'agent travaille donc principalement sur une représentation symbolique fiable.

---

# 56. Visual feedback

Le système peut néanmoins permettre à un modèle vision d'analyser un preview.

```text
SceneGraph
    ↓
Render Preview
    ↓
Vision Model
    ↓
Composition critique
```

Exemple :

```text
"The man's silhouette merges with the bookshelf."
```

L'agent peut alors proposer :

```text
Move man 20 cm left
```

---

# 57. Scene Evaluator

Un composant spécifique peut analyser automatiquement :

```text
occlusion
silhouette readability
subject visibility
camera clipping
lighting
pose balance
composition
```

Exemple :

```json
{
  "issues": [
    {
      "type": "occlusion",
      "severity": "medium",
      "subjects": ["man", "door"]
    }
  ]
}
```

---

# 58. Agent architecture

Architecture proposée :

```text
User Prompt
    │
    ▼
Illustration Agent
    │
    ├── Scene Planner
    │
    ├── Asset Resolver
    │
    ├── Pose Planner
    │
    ├── Composition Agent
    │
    └── Render Controller
    │
    ▼
SceneGraph
```

Il n'est pas nécessaire que tous ces éléments soient des agents LLM autonomes.

Ils peuvent être des composants spécialisés orchestrés par l'agent principal.

---

# 59. Scene Planner

Responsabilités :

```text
prompt
 ↓
entities
 ↓
relationships
 ↓
environment
 ↓
initial composition
```

Sortie :

```text
SceneIntent
```

---

# 60. Asset Resolver

Entrée :

```text
SceneIntent object
```

Exemple :

```text
"old wooden bookstore counter"
```

Sortie :

```text
asset://furniture/counter_victorian_02
```

Le resolver doit expliquer son choix à l'agent.

---

# 61. Pose Planner

Entrée :

```text
man entering bookstore
```

Sortie :

```yaml
pose:
  base: walking

  left_hand:
    target: door_handle

  head:
    look_at: cat

  torso:
    rotation: 12deg
```

Puis le solveur IK calcule la pose réelle.

---

# 62. Composition Agent

Il reçoit :

```text
SceneIntent
+
SceneGraph
```

et détermine :

```text
camera position
lens
subject placement
visual hierarchy
```

Le résultat reste une suggestion modifiable.

---

# 63. Scene history

Toutes les modifications doivent produire des opérations.

Exemple :

```json
{
  "operation": "rig.look_at",
  "node": "man",
  "target": "cat"
}
```

Cela permet :

```text
undo
redo
history
agent audit
```

---

# 64. Operation Log

Exemple :

```text
14:32 User    created project
14:32 Agent   generated scene
14:33 Agent   placed counter
14:33 Agent   placed cat
14:34 User    moved cat
14:35 User    rotated camera
14:36 Agent   changed man's gaze
14:38 User    rendered pencil preview
```

Cela s'intègre particulièrement bien à la philosophie de traçabilité d'Akasha OS.

---

# 65. Project structure

Projet local :

```text
illustration-project/
│
├── project.yaml
├── scene.yaml
├── history.log
│
├── assets/
│
├── styles/
│
├── previews/
│
└── renders/
```

---

# 66. project.yaml

```yaml
id: bookstore-cat

name: Bookstore Cat

created: 2026-09-21

scene:
  file: scene.yaml

active_camera: camera_main

render:
  style: pencil_classic

history:
  enabled: true
```

---

# 67. Autosave

Chaque opération modifiant la scène déclenche un autosave différé.

Objectif :

```text
maximum data loss < 5 seconds
```

Les rendus ne modifient jamais automatiquement la scène.

---

# 68. Scene snapshots

L'utilisateur peut créer :

```text
Snapshot A
Snapshot B
Snapshot C
```

Exemple :

```text
Composition A
Composition B
Close-up
Wide shot
```

Ils référencent les mêmes assets.

---

# 69. Variants

Une évolution peut permettre :

```text
Scene
├── Variant A
├── Variant B
└── Variant C
```

avec stockage différentiel.

Exemple :

```text
Variant A:
camera = Camera01

Variant B:
camera = Camera02
man.pose = pose_04
```

---

# 70. Akasha capabilities

Le module doit respecter le modèle de sécurité capability-based d'Akasha.

Capabilities possibles :

```text
gpu.render
```

```text
fs.read:/illustrations/**
```

```text
fs.write:/illustrations/**
```

```text
asset.read
```

```text
asset.install
```

et éventuellement :

```text
network.fetch
```

pour récupérer des assets externes.

---

# 71. Render backend capability

L'accès au moteur Blender ne doit pas équivaloir à donner un accès arbitraire aux processus système.

Prévoir une capability :

```text
render.execute
```

Akasha appelle un service contrôlé :

```text
RenderService
```

plutôt que :

```text
shell.exec("blender ...")
```

---

# 72. Render Service

Interface logique :

```text
render.submit
render.status
render.cancel
render.result
```

Exemple :

```json
{
  "scene": "scene://bookstore",

  "camera": "camera_main",

  "preset": "pencil_preview",

  "resolution": [1024, 1024]
}
```

---

# 73. Isolation

Le renderer reçoit uniquement :

```text
scene package
assets
render parameters
output directory
```

Il ne doit pas avoir accès arbitrairement :

```text
home directory
network
credentials
Akasha vault
other projects
```

---

# 74. External assets

Chaque asset possède :

```text
origin
license
hash
version
```

Exemple :

```yaml
asset:
  id: old_counter

  origin: local-library

  sha256: ...

  license: CC0

  version: 1
```

Cela rend les projets reproductibles.

---

# 75. Missing asset

Si un projet référence un asset absent :

```text
asset missing
```

Akasha affiche un placeholder.

```text
┌───────────────┐
│               │
│ MISSING ASSET │
│               │
└───────────────┘
```

L'utilisateur peut demander :

```text
Find replacement
```

---

# 76. Offline-first

Le module doit fonctionner sans Internet.

Le package de base peut fournir une bibliothèque minimale :

```text
humans
cats
dogs

chairs
tables
doors
windows

basic architecture

basic vegetation
```

Les packs supplémentaires sont optionnels.

---

# 77. Asset LOD

Les assets devraient posséder plusieurs niveaux.

```text
LOD0
LOD1
LOD2
```

Le viewport utilise une version légère.

Le renderer utilise la meilleure version disponible.

---

# 78. Proxy geometry

Lors de la génération initiale, l'Asset Resolver peut ne pas avoir encore chargé les assets.

Le système utilise alors :

```text
proxy geometry
```

Exemple :

```text
human → mannequin
cat → quadruped proxy
counter → box
bookshelf → box
```

La composition devient donc visible presque immédiatement.

---

# 79. Progressive scene generation

C'est un point important pour l'UX.

Au lieu de :

```text
Prompt
↓
wait 30 seconds
↓
complete scene
```

faire :

```text
Prompt
 ↓
SceneIntent
 ↓
proxy scene
 ↓
camera
 ↓
characters
 ↓
assets
 ↓
materials
 ↓
lighting
```

L'utilisateur voit la scène se construire progressivement.

---

# 80. Scene generation target

Objectif UX :

```text
Prompt → proxy scene
```

en quelques secondes.

Les assets détaillés peuvent arriver ensuite.

---

# 81. GPU architecture

Akasha disposant déjà d'une abstraction GPU, `scene3d` devrait idéalement s'intégrer à cette infrastructure.

Deux workloads doivent être distingués :

```text
interactive viewport
```

et :

```text
offline rendering
```

---

# 82. Viewport

Objectif :

```text
30–60 FPS
```

sur une scène moyenne.

Le viewport n'a pas besoin de produire le style final exact.

Il doit privilégier :

```text
responsiveness
selection
posing
camera
composition
```

---

# 83. NPR Preview

Une approximation temps réel peut afficher :

```text
toon shading
outline
flat colors
paper overlay
```

afin de donner une idée du rendu final.

---

# 84. CPU fallback

Le module doit rester utilisable sans GPU puissant.

Mode :

```text
CPU / low-spec
```

avec :

```text
reduced viewport quality
reduced shadows
proxy geometry
lower preview resolution
CPU final rendering
```


---

# 85. Hardware profiles

Le module doit adapter automatiquement sa qualité aux capacités de la machine.

```yaml id="x33j2c"
hardware_profiles:

  low:
    viewport:
      shadows: false
      max_lod: 2
      max_texture_size: 1024
    render:
      preview_resolution: 512
      samples: 16

  medium:
    viewport:
      shadows: true
      max_lod: 1
      max_texture_size: 2048
    render:
      preview_resolution: 1024
      samples: 32

  high:
    viewport:
      shadows: true
      max_lod: 0
      max_texture_size: 4096
    render:
      preview_resolution: 1536
      samples: 64
```

Akasha peut sélectionner automatiquement le profil en fonction des informations fournies par son service GPU.

L'utilisateur conserve la possibilité de le modifier.

---

# 86. Resource budgets

Une scène doit avoir un budget connu.

Exemple :

```yaml id="k4u3tr"
budget:

  triangles:
    viewport: 2_000_000

  textures:
    memory_mb: 2048

  characters:
    active_rigs: 10

  lights:
    dynamic: 8
```

Si la scène dépasse le budget, le runtime peut :

```text id="q4jnlz"
reduce LOD
disable secondary shadows
stream assets
freeze distant rigs
```

---

# 87. Scene3D Runtime Architecture

Le widget `scene3d` ne doit pas contenir toute la logique 3D.

Architecture proposée :

```text id="xsk7md"
declarative_ui
      │
      ▼
scene3d widget
      │
      ▼
Scene Runtime
      │
 ┌────┼────────────┐
 ▼    ▼            ▼
Scene Rig       Asset
Graph  Solver    Manager
 │      │            │
 └──────┼────────────┘
        ▼
   GPU Renderer
```

Le widget est essentiellement une vue interactive du `Scene Runtime`.

---

# 88. Scene Runtime

Responsabilités :

```text id="atykva"
SceneGraph lifecycle
transforms
hierarchy
selection
visibility
cameras
lights
rigging
animation state
asset references
```

Il ne doit pas contenir de logique propre à `Illustration Studio`.

---

# 89. Runtime API

Interface conceptuelle :

```text id="m4gtv6"
scene.open
scene.close

scene.node.create
scene.node.delete
scene.node.update

scene.transform.set

scene.selection.set

scene.camera.set_active

scene.rig.set_pose

scene.snapshot
```

Cette API est accessible :

```text id="58ouzi"
UI
Agent tools
Module
```

avec des capabilities différentes.

---

# 90. Declarative events

Le widget peut produire des événements.

```yaml id="mmsxux"
events:

  on_select:
    emit: scene.selection.changed

  on_transform:
    emit: scene.node.transformed

  on_pose_change:
    emit: scene.rig.changed

  on_camera_change:
    emit: scene.camera.changed
```

Cela permet au module de réagir sans exécuter de JavaScript arbitraire.

---

# 91. Scene3D Commands

La declarative UI peut également envoyer des commandes.

```yaml id="66azje"
actions:

  reset_camera:
    command: scene.camera.reset

  frame_selection:
    command: scene.camera.frame_selection

  toggle_skeleton:
    command: scene.rig.toggle_skeleton
```

On reste donc dans le modèle fermé et contrôlé d'Akasha.

---

# 92. IPC

Les échanges module/runtime doivent utiliser l'IPC sémantique d'Akasha.

Conceptuellement :

```text id="v21kg3"
illustration-studio
        │
        │ semantic IPC
        ▼
scene-service
        │
        ├── asset-service
        ├── gpu-service
        └── render-service
```

Aucun composant n'obtient implicitement accès aux autres.

---

# 93. Services

À terme, quatre services génériques sont intéressants.

```text id="tmzyl4"
scene-service
asset-service
rig-service
render-service
```

Ils peuvent être réutilisés par d'autres applications Akasha.

---

# 94. Module Architecture

Le module lui-même peut être organisé ainsi :

```text id="kkfnhz"
illustration-studio/
│
├── manifest.yaml
├── ui.yaml
│
├── tools/
│   ├── scene.yaml
│   ├── pose.yaml
│   ├── composition.yaml
│   └── render.yaml
│
├── planners/
│   ├── scene-planner.yaml
│   ├── pose-planner.yaml
│   └── composition-planner.yaml
│
├── schemas/
│   ├── scene-intent.schema.json
│   ├── project.schema.json
│   └── style.schema.json
│
├── styles/
│   ├── sketch.yaml
│   ├── pencil.yaml
│   ├── ink.yaml
│   └── marker.yaml
│
└── assets/
    └── defaults/
```

Les détails exacts devront évidemment suivre le format réel du SDK module Akasha.

---

# 95. Module manifest

Conceptuellement :

```yaml id="cbj0k6"
module:

  id: illustration-studio

  name: Illustration Studio

  version: 0.1.0

  ui:
    type: declarative

  requires:

    runtime:
      - scene3d

    services:
      - scene
      - assets
      - render

  capabilities:
    - gpu.render
    - asset.read
    - fs.read:/illustrations/**
    - fs.write:/illustrations/**
```

---

# 96. Project creation flow

Workflow complet :

```text id="hs57ob"
New Illustration
      │
      ▼
Prompt
      │
      ▼
Scene Planner
      │
      ▼
SceneIntent
      │
      ▼
Proxy Scene
      │
      ▼
Asset Resolver
      │
      ▼
Detailed Scene
      │
      ▼
Pose Planner
      │
      ▼
Composition
      │
      ▼
User Editing
      │
      ▼
Style
      │
      ▼
Render
```

---

# 97. UX : New Illustration

Premier écran :

```text id="6tzovv"
┌─────────────────────────────────────────────────┐
│                                                 │
│              Create an illustration             │
│                                                 │
│  Describe your scene                            │
│                                                 │
│  ┌───────────────────────────────────────────┐  │
│  │ An old bookstore. A man enters through   │  │
│  │ the door while a cat lies on the counter │  │
│  │ and watches him.                         │  │
│  └───────────────────────────────────────────┘  │
│                                                 │
│  Aspect ratio                                   │
│                                                 │
│  [ 16:9 ▼ ]                                     │
│                                                 │
│                       [ Create Scene ]           │
│                                                 │
└─────────────────────────────────────────────────┘
```

Le choix du style n'est pas obligatoire à cette étape.

---

# 98. Scene generation feedback

Pendant la génération :

```text id="19urqb"
Understanding scene...

✓ Environment: old bookstore
✓ Character: man
✓ Animal: cat
✓ Main prop: counter

Building composition...

✓ Camera
✓ Proxy environment
✓ Character placement

Finding assets...

● Victorian bookshelf
● Wooden counter
● Male character
● Cat
```

La scène apparaît progressivement dans le viewport.

---

# 99. Prompt preservation

Le prompt initial doit rester attaché au projet.

```yaml id="9uixmt"
intent:

  original_prompt: >
    An old bookstore. A man enters through
    the door while a cat lies on the counter.

  generated_at: ...

  planner_version: ...
```

Cela permet de reconstruire ou réinterpréter la scène.

---

# 100. Regeneration

L'utilisateur peut demander :

```text id="39ob78"
Regenerate scene
```

mais plusieurs niveaux doivent être proposés :

```text id="k9e8bb"
Regenerate composition

Regenerate environment

Regenerate poses

Find different assets

Start over
```

On évite ainsi de détruire inutilement le travail manuel.

---

# 101. Locking

L'utilisateur doit pouvoir verrouiller des éléments.

Exemple :

```text id="e32i7j"
🔒 Camera
🔒 Cat pose
🔓 Man
🔓 Lighting
```

Si l'utilisateur demande :

> Recompose la scène.

l'agent ne peut pas modifier les éléments verrouillés.

---

# 102. Agent constraints

Les locks deviennent des contraintes du planner.

```json id="j8jzmj"
{
  "locked": [
    "camera_main",
    "cat.pose"
  ]
}
```

Cela est essentiel pour un workflow humain/IA fiable.

---

# 103. Pinning semantic decisions

On peut également verrouiller des décisions sémantiques.

Exemple :

```text id="4u3hs6"
Cat must remain on counter

Man must enter through door

Camera must remain low angle
```

Ces contraintes persistent lors des régénérations.

---

# 104. Composition guides

Le viewport peut afficher :

```text id="3ul2rs"
rule of thirds
golden ratio
center lines
safe areas
horizon
perspective guides
```

Ces guides ne sont jamais rendus dans l'image finale.

---

# 105. Drawing-specific camera

Pour l'illustration, il peut être intéressant d'autoriser des projections moins réalistes.

En plus de :

```text id="5i79nz"
perspective
orthographic
```

prévoir ultérieurement :

```text id="x6ktav"
weak perspective
isometric
2.5D illustration
```

---

# 106. Exaggeration

L'illustration ne doit pas être contrainte au réalisme physique.

On doit pouvoir exagérer :

```text id="x2vbfq"
perspective
character proportions
pose
silhouette
lighting
```

Le SceneGraph doit donc éviter des contraintes inutilement réalistes.

---

# 107. Stylized characters

Les rigs normalisés doivent pouvoir fonctionner avec :

```text id="h0x05r"
realistic
semi-realistic
cartoon
chibi
children-book
```

La topologie du mesh importe moins que la compatibilité du rig.

---

# 108. Character templates

Exemple :

```text id="m79dxq"
characters/

human/
├── male_generic
├── female_generic
├── child_generic
├── cartoon_adult
└── cartoon_child
```

L'apparence pourra ensuite être personnalisée.

---

# 109. Character customization

V1 :

```text id="7suxip"
height
body proportions
hair
skin tone
clothing
basic colors
```

V2 :

```text id="ht9z95"
face customization
procedural clothing
custom character generation
```

---

# 110. 3D generation

La génération IA d'assets 3D ne doit **pas être une dépendance du MVP**.

Le MVP repose sur :

```text id="ezuv1v"
curated asset library
+
semantic retrieval
+
procedural primitives
```

C'est beaucoup plus prévisible.

---

# 111. Future AI 3D generation

Plus tard :

```text id="fq6czm"
Prompt
 ↓
Asset Search
 ↓
No suitable asset
 ↓
3D Generator
 ↓
Mesh
 ↓
Validation
 ↓
Optimization
 ↓
Asset Library
```

Le résultat doit passer par une validation avant d'entrer dans la scène.

---

# 112. Asset validation

Contrôles :

```text id="wcexlj"
polygon count
bounding box
normals
UV
materials
textures
rig compatibility
scale
license metadata
```

---

# 113. Procedural assets

Certains objets sont plus intéressants à générer procéduralement.

Exemple :

```text id="7xslrx"
bookshelf
books
walls
floor
stairs
windows
tables
roads
terrain
```

Une librairie peut ainsi être composée à partir de paramètres :

```yaml id="7nsh0f"
bookshelf:

  width: 3
  height: 2.5

  shelves: 6

  books:
    density: 0.8
```

---

# 114. Environment generators

À terme :

```text id="3cm6nf"
room.generate
street.generate
forest.generate
shop.generate
terrain.generate
```

Ces generators produisent toujours du SceneGraph standard.

---

# 115. Illustration-specific geometry

Un avantage important de cette approche est que la géométrie n'a pas besoin d'être parfaite.

Pour une image fixe :

```text id="11nh4l"
camera-visible geometry
```

est plus importante que :

```text id="nbmxyg"
complete realistic environment
```

Le Scene Planner peut donc optimiser la scène par rapport à la caméra.

---

# 116. Camera-aware generation

Exemple :

La caméra ne voit jamais l'arrière de la librairie.

Le système peut utiliser :

```text id="0vk8ju"
partial environment
```

plutôt qu'une librairie complète.

Cela réduit fortement :

```text id="mnzlkb"
generation time
VRAM
asset count
render cost
```

---

# 117. Render passes

Le backend doit idéalement produire :

```text id="p0cq10"
beauty
depth
normal
albedo
shadow
object_id
material_id
line
ambient_occlusion
```

Ces passes peuvent être réutilisées par différents styles.

---

# 118. Style independence

Ainsi :

```text id="8m0kln"
3D Render Passes
       │
       ├── Pencil
       ├── Ink
       ├── Comic
       ├── Watercolor
       └── Marker
```

Il n'est pas nécessaire de recalculer toute la géométrie pour chaque style.

---

# 119. Render cache

Les passes peuvent être mises en cache.

Clé :

```text id="oemh0w"
scene_hash
+
camera_hash
+
lighting_hash
+
geometry_settings
```

Si seul le style change :

```text id="14pvti"
reuse render passes
```

---

# 120. Example

L'utilisateur génère :

```text id="jhr2yk"
Pencil
```

puis demande :

```text id="sx64nq"
Show it in ink.
```

Si la scène n'a pas changé :

```text id="as9ucj"
NO geometry render
```

On exécute uniquement :

```text id="d6uj6f"
NPR post-processing
```

---

# 121. Render job model

```yaml id="2vs8r3"
job:

  id: render_001

  scene_revision: 42

  camera: camera_main

  preset: pencil_preview

  status: rendering

  progress: 0.64
```

---

# 122. Background rendering

Les rendus longs doivent fonctionner comme tâches Akasha.

```text id="rxxgjp"
Render
  ↓
Background Task
  ↓
user continues editing
```

Attention : le job conserve la révision de scène utilisée.

---

# 123. Reproducibility

Un rendu doit enregistrer :

```text id="dkgrxu"
scene revision
camera
style
renderer version
render settings
asset versions
random seed
```

Cela permet de reproduire exactement une illustration.

---

# 124. Export metadata

Option :

```text id="vs0f4u"
illustration.png
illustration.metadata.json
```

Metadata :

```json id="8nlhrz"
{
  "project": "bookstore-cat",
  "sceneRevision": 42,
  "style": "pencil_classic",
  "renderer": "blender-npr",
  "rendererVersion": "..."
}
```

---

# 125. Error handling

Les erreurs doivent être localisées.

Exemple :

```text id="s9a9ms"
Asset failed
```

ne doit pas provoquer :

```text id="6r20s9"
Scene failed
```

Le système remplace l'asset par un proxy.

---

# 126. Renderer crash

Si Blender ou un autre renderer plante :

```text id="m70n0e"
RenderService
   ↓
renderer crash
   ↓
job failed
```

mais :

```text id="67g8qb"
SceneGraph remains intact
```

Le module peut proposer :

```text id="o0z01e"
Retry

Retry in safe mode

Lower quality

View logs
```

---

# 127. Agent failure

Si le Scene Planner produit un résultat invalide :

```text id="4iqam7"
Schema validation
      ↓
repair attempt
      ↓
fallback
```

Jamais de SceneGraph arbitrairement invalide.

---

# 128. Schema validation

Tous les objets importants doivent avoir un JSON Schema.

```text id="0guzva"
SceneIntent.schema

SceneGraph.schema

RenderPreset.schema

Style.schema

Project.schema
```

---

# 129. Undo / Redo architecture

Les opérations doivent être command-based.

Exemple :

```text id="bs5ylh"
MoveNodeCommand
```

avec :

```text id="e1j0yp"
apply()
undo()
```

Cela simplifie :

```text id="5c6vfj"
undo
redo
history
collaboration
agent audit
```

---


# 130. Concurrency

Si l'agent modifie la scène pendant que l'utilisateur manipule un objet, il faut éviter les conflits.

Première stratégie :

```text id="v4i2zq"
node editing lock
```

Exemple :

```text id="jtv6dz"
User starts dragging man
        ↓
man locked by UI
        ↓
Agent cannot modify man.transform
        ↓
User releases man
        ↓
lock released
```

Le verrou doit être très court et limité au composant manipulé.

---

# 131. Semantic locks vs editing locks

Il faut distinguer deux types de locks.

### Editing lock

Temporaire :

```text id="1m4ujk"
user currently editing node
```

### Semantic lock

Persistant :

```text id="35xlwc"
user does not want agent to change node
```

Exemple :

```yaml id="w0sug9"
locks:

  camera_main:
    semantic: true

  cat:
    pose: true
```

---

# 132. Agent transactions

Les modifications complexes de l'agent doivent être transactionnelles.

Exemple :

> Fais regarder l'homme vers le chat et mets sa main gauche sur la poignée.

peut produire :

```text id="eq7u05"
BEGIN TRANSACTION

rig.look_at(man, cat)

rig.solve_ik(
    man.left_hand,
    door.handle
)

validate_pose()

COMMIT
```

Si la validation échoue :

```text id="v5zv81"
ROLLBACK
```

La scène ne doit jamais rester dans un état partiellement modifié.

---

# 133. Preview before apply

Pour les changements importants, l'agent peut produire une proposition.

```text id="prk3wa"
Current Scene
     │
     ├───────────────┐
     │               │
     ▼               ▼
 Current          Proposal
 Scene             Scene
                       │
                       ▼
                   Preview
```

L'utilisateur choisit :

```text id="pphmbe"
Apply

Discard

Modify
```

Ce mécanisme peut être particulièrement utile pour :

```text id="ms0wpc"
composition changes
large pose changes
environment regeneration
asset replacement
lighting redesign
```

---

# 134. Scene branches

Une évolution naturelle consiste à autoriser :

```text id="r3egpc"
Scene
 ├── Main
 ├── Proposal A
 └── Proposal B
```

Les propositions de l'agent deviennent alors des branches temporaires du SceneGraph.

---

# 135. Multi-agent compatibility

Même si le MVP utilise un agent principal, l'architecture doit permettre :

```text id="8hv9bf"
Director Agent
      │
      ├── Scene Agent
      ├── Pose Agent
      ├── Camera Agent
      ├── Lighting Agent
      └── Rendering Agent
```

Chaque agent reçoit uniquement les capabilities nécessaires.

Exemple :

```text id="yqkr8d"
Pose Agent
```

peut modifier :

```text id="86ob13"
character rigs
```

mais pas :

```text id="4dvpu1"
assets
filesystem
renderer configuration
```

---

# 136. Illustration Director

À terme, un rôle intéressant est celui de :

```text id="2j6tue"
Illustration Director
```

Il analyse :

```text id="kzqavh"
prompt
scene
composition
poses
lighting
style
```

et coordonne les modifications.

Il ne manipule pas nécessairement directement les données.

---

# 137. Natural language iteration

Exemple complet :

```text id="nbgcr6"
USER

The scene feels too static.
```

Le Director peut interpréter :

```text id="3h50p6"
man pose too neutral
camera too frontal
cat/man interaction weak
```

et proposer :

```text id="9zjigj"
- rotate man's torso
- shift weight forward
- turn head toward cat
- lower camera
- move camera 20° right
```

L'utilisateur peut accepter tout ou partie.

---

# 138. Explainability

Lorsque l'agent effectue une modification importante, il doit pouvoir expliquer simplement ce qu'il a changé.

Exemple :

```text id="rlp9b5"
I moved the camera slightly lower and to the right,
changed the lens from 50 mm to 35 mm and rotated
the man's torso toward the cat.
```

L'historique conserve les opérations techniques.

---

# 139. MVP Philosophy

Le MVP ne doit **pas** chercher à concurrencer Blender.

Le but est de valider :

```text id="8jkm0j"
Prompt
→
Editable 3D composition
→
Pose editing
→
Illustration rendering
```

Tout ce qui n'est pas indispensable à cette boucle doit être repoussé.

---

# 140. MVP Scope

Le MVP doit supporter :

### Scene

```text id="yrqzgc"
SceneGraph
transforms
hierarchy
meshes
characters
camera
lights
```

### UI

```text id="s8a2a4"
scene3d viewport
selection
translate
rotate
scale
scene tree
properties
```

### Characters

```text id="03vt70"
humanoid rig
basic quadruped rig
IK hands
IK feet
look-at
pose presets
```

### AI

```text id="9d42ec"
prompt → SceneIntent
SceneIntent → SceneGraph
asset selection
initial poses
initial camera
```

### Render

```text id="3wxt0m"
Blender headless
line extraction
basic NPR
```

### Styles

```text id="9sm0aq"
Sketch
Pencil
Ink
```

### Project

```text id="8fn1w0"
save
load
autosave
undo
redo
PNG export
```

---

# 141. Explicitly outside MVP

Ne pas mettre dans le MVP :

```text id="n3e3nl"
AI-generated 3D meshes

advanced character creator

facial animation

animation timeline

cloth simulation

physics simulation

advanced procedural environments

watercolor simulation

PSD export

real-time collaboration

community marketplace
```

Ces fonctionnalités augmenteraient fortement le périmètre sans valider davantage le concept principal.

---

# 142. MVP Asset Pack

Pour tester correctement le concept, fournir un petit asset pack contrôlé.

### Characters

```text id="rt9xhu"
adult male
adult female
child
cat
dog
```

### Furniture

```text id="8utd0w"
table
chair
counter
bookshelf
sofa
desk
```

### Architecture

```text id="n1bnlf"
wall
floor
door
window
stairs
```

### Props

```text id="6ybicf"
book
lamp
cup
box
plant
```

L'objectif n'est pas la quantité mais la cohérence.

---

# 143. MVP demonstration scene

La scène de référence peut justement être :

> An old bookstore. A man enters through the door while a cat lies on the counter watching him.

Elle teste :

```text id="gkrp9e"
environment generation
human
animal
object relationships
pose
look-at
camera
lighting
NPR
```

Elle constitue donc un excellent scénario end-to-end.

---

# 144. MVP Acceptance Scenario

### Step 1

L'utilisateur saisit le prompt.

### Step 2

En quelques secondes, une scène proxy apparaît.

### Step 3

Les assets détaillés remplacent progressivement les proxies.

### Step 4

L'homme est placé près de la porte.

### Step 5

Le chat est placé sur le comptoir.

### Step 6

L'utilisateur sélectionne l'homme.

### Step 7

Le squelette apparaît.

### Step 8

L'utilisateur déplace sa main.

### Step 9

IK ajuste le bras.

### Step 10

L'utilisateur tourne la tête vers le chat.

### Step 11

Il déplace légèrement la caméra.

### Step 12

Il sélectionne :

```text id="iwz2s8"
Pencil
```

### Step 13

Un preview est généré.

### Step 14

Il sélectionne :

```text id="4k70m1"
Ink
```

### Step 15

Le nouveau style réutilise les passes compatibles.

### Step 16

Il exporte le résultat en PNG.

Si ce scénario fonctionne correctement, le MVP valide le concept.

---

# 145. MVP Acceptance Criteria

## Scene generation

Le système doit être capable de transformer un prompt simple contenant :

```text id="y3tdgg"
environment
1–3 characters
5–20 objects
spatial relationships
```

en SceneGraph valide.

---

## Editing

L'utilisateur doit pouvoir :

```text id="r2ib2w"
select
move
rotate
scale
```

un objet sans intervention de l'agent.

---

## Rig

L'utilisateur doit pouvoir modifier une pose directement dans le viewport.

---

## Agent

L'utilisateur doit pouvoir demander :

> Make the man look at the cat.

et constater la modification dans la scène.

---

## Camera

La caméra doit être entièrement repositionnable.

---

## Rendering

Au moins trois styles doivent être visuellement distincts :

```text id="tfmbi7"
Sketch
Pencil
Ink
```

---

## Persistence

Après fermeture/réouverture :

```text id="bupyb4"
scene
poses
camera
lights
style
```

doivent être restaurés.

---

# 146. Performance targets

MVP cible desktop.

### Proxy scene

Objectif :

```text id="xjklwa"
< 3 seconds
```

après obtention du SceneIntent.

### Scene interaction

Objectif :

```text id="3hl5da"
>= 30 FPS
```

sur machine recommandée.

### Pose interaction

Objectif :

```text id="u70a8e"
< 50 ms
```

de latence perceptible.

### Preview

Objectif indicatif :

```text id="ggspio"
1024 × 1024
< 10 seconds
```

sur GPU compatible.

Le CPU fallback peut être significativement plus lent.

---

# 147. Phase 0 — Runtime prototype

Avant le module complet, créer un prototype :

```text id="g5e37p"
scene3d-lab
```

Il doit uniquement afficher :

```text id="aqsm9u"
cube
camera
light
rigged mannequin
```

et permettre :

```text id="c24u4f"
orbit
selection
transform
bone manipulation
```

Pas d'IA.

Pas de rendu NPR.

---

# 148. Phase 0 Exit Criteria

On ne poursuit que si Akasha peut héberger proprement une application 3D interactive sans WebView.

Critères :

```text id="wz3hj8"
stable viewport
declarative integration
GPU integration
scene events
selection
transform gizmos
rig manipulation
capability isolation
```

C'est probablement le **principal risque architectural du projet**.

---

# 149. Phase 1 — Scene Core

Implémenter :

```text id="pkx3x7"
SceneGraph
Scene Runtime
asset loading
camera
lights
transform
serialization
```

Livrable :

```text id="xt88je"
editable static 3D scene
```

---

# 150. Phase 2 — Rigging

Implémenter :

```text id="kk8n93"
humanoid rig
quadruped rig
FK
IK
look-at
pose presets
```

Livrable :

```text id="s2qnm8"
interactive character posing
```

---

# 151. Phase 3 — Illustration Module

Créer :

```text id="nn9l2i"
illustration-studio
```

avec :

```text id="okv51k"
project UI
scene tree
properties
pose mode
camera mode
style mode
```

---

# 152. Phase 4 — Scene AI

Ajouter :

```text id="95ljj4"
Prompt
 ↓
SceneIntent
 ↓
SceneGraph
```

ainsi que :

```text id="f7v2yq"
Asset Resolver
Pose Planner
Composition Planner
```

---

# 153. Phase 5 — NPR

Intégrer :

```text id="o6k2c9"
RenderService
+
Blender headless
```

Premiers styles :

```text id="h0fz3e"
Sketch
Pencil
Ink
```

---

# 154. Phase 6 — Agent editing

Ajouter les tools :

```text id="0ij3np"
scene.*
rig.*
camera.*
light.*
composition.*
render.*
```

L'agent Akasha peut alors travailler directement dans le projet.

---

# 155. Version 1.0

Après validation du MVP :

```text id="dhbtci"
better asset library
advanced composition
better NPR
marker
charcoal
character customization
facial expressions
style packs
render caching
scene variants
```

---

# 156. Version 1.5

Ajouter :

```text id="zh7i2h"
procedural rooms
procedural streets
procedural forests

better animals

clothing variants

advanced camera tools

visual composition evaluator
```

---

# 157. Version 2.0

Ajouter éventuellement :

```text id="gfw8jd"
AI 3D asset generation

AI character generation

advanced watercolor

painting styles

community assets

community styles

custom render backends
```

---

# 158. Future: 2D drawing layer

Une évolution particulièrement intéressante consiste à combiner :

```text id="5tttkp"
scene3d
+
Akasha Canvas
```

Le rendu 3D devient une base pour le dessin manuel.

Architecture :

```text id="v5vhfl"
3D Scene
   ↓
NPR Render
   ↓
Canvas
   ↓
Manual drawing
```

---

# 159. Hybrid 2D/3D workflow

On pourrait aller plus loin :

```text id="bjsa0g"
Canvas
├── Background Render
├── Line Art
├── Color
├── Shadows
└── User Drawing
```

L'utilisateur pourrait alors dessiner au-dessus de la composition.

Cela transformerait Illustration Studio en véritable environnement d'illustration assistée.

---

# 160. Vector line export

Le Line Art pourrait être exporté sous forme vectorielle.

```text id="hwqmby"
3D Geometry
 ↓
Line Extraction
 ↓
Vector Paths
 ↓
Akasha Canvas
```

L'utilisateur pourrait modifier les traits individuellement.

C'est particulièrement intéressant pour :

```text id="4q1dyf"
comics
manga
technical illustration
children books
storyboards
```

---

# 161. Storyboard mode

Le même système peut évoluer naturellement vers :

```text id="a7d4ui"
Storyboard Studio
```

avec plusieurs shots partageant la même scène.

```text id="ur9vsc"
Scene
├── Shot 01
├── Shot 02
├── Shot 03
└── Shot 04
```

Chaque shot stocke :

```text id="8a9lrb"
camera
character poses
visibility
lighting overrides
```

---

# 162. Comic mode

Une autre évolution :

```text id="td3mzg"
Comic Project
│
├── Page 1
│   ├── Panel 1
│   ├── Panel 2
│   └── Panel 3
│
└── Page 2
```

Chaque panel référence une scène/shot.

Le rendu peut être envoyé vers Akasha Canvas pour :

```text id="kh1l2m"
speech bubbles
text
manual corrections
layout
```

---

# 163. Animation compatibility

Même si l'animation est hors MVP, le SceneGraph doit éviter de bloquer cette possibilité.

À terme :

```text id="7b4iyg"
pose
```

peut devenir :

```text id="g8qrdk"
animation track
```

et :

```text id="zzkqq7"
camera
```

peut avoir une timeline.

Cela ouvrirait :

```text id="6r0hwh"
animatics
animated storyboards
stylized animation
```

---

# 164. Why not build on Blender UI directly?

Utiliser directement Blender serait plus rapide techniquement mais irait à l'encontre du produit.

L'utilisateur devrait apprendre :

```text id="dv4ghy"
Blender concepts
complex UI
modifiers
nodes
collections
render engines
```

Illustration Studio doit exposer uniquement les concepts nécessaires :

```text id="42xwcz"
Scene
Character
Pose
Camera
Light
Style
Render
```

Blender reste un détail d'implémentation.

---

# 165. Why not image generation?

La génération d'image classique donne :

```text id="v2o40n"
Prompt
 ↓
Pixels
```

Modifier précisément :

```text id="3im5i9"
left hand
camera focal length
cat position
lighting direction
```

est difficile et souvent non déterministe.

Ici :

```text id="b12cjr"
Prompt
 ↓
Structured World
 ↓
Controlled Rendering
```

Chaque élément est explicitement manipulable.

---

# 166. Optional generative post-process

Cela n'interdit cependant pas complètement les modèles génératifs.

À terme :

```text id="w9a7oj"
3D Render
+
Depth
+
Normals
+
Line Art
+
Style Prompt
      ↓
Generative Refinement
```

pourrait améliorer certains styles.

Mais cette étape doit rester :

```text id="7j4vd6"
optional
```

et ne jamais remplacer la scène source.

---

# 167. Structural conditioning

Si un modèle génératif est utilisé, il doit être fortement conditionné par :

```text id="1h6q3d"
depth
normal
segmentation
line art
pose
```

afin de préserver la composition.

Le résultat génératif est un **render derivative**, jamais le projet.

---

# 168. Core architectural decision

La décision architecturale principale est la suivante :

> `Illustration Studio` ne doit pas introduire un moteur 3D monolithique spécifique au module.

Il doit introduire une capacité générique Akasha :

```text id="kq1bb1"
Scene Runtime
+
scene3d widget
```

sur laquelle le module d'illustration vient se construire.

Architecture cible :

```text id="syrqv8"
                    AKASHA OS

                       │
              Declarative UI
                       │
                       ▼
                ┌────────────┐
                │  scene3d   │
                └─────┬──────┘
                      │
                      ▼
              ┌───────────────┐
              │ Scene Runtime │
              └───────┬───────┘
                      │
       ┌──────────────┼──────────────┐
       │              │              │
       ▼              ▼              ▼
 Asset Service    Rig Service    GPU Service
       │              │              │
       └──────────────┼──────────────┘
                      │
                      ▼
               Render Service
                      │
             ┌────────┴────────┐
             ▼                 ▼
          Blender           Future
           NPR              Backend
```

`Illustration Studio` devient alors un consommateur de ces services.

---

# 169. Architectural consequence

Cette décision transforme `scene3d` en **primitive système Akasha**.

Au même titre qu'une primitive UI peut afficher :

```text id="4q7yvl"
text
button
table
form
canvas
```

Akasha devient capable d'afficher :

```text id="u1u45g"
scene3d
```

Ce changement dépasse largement Illustration Studio.

---

# 170. Rich Application Foundation

Cette évolution répond à un besoin plus général déjà identifié pour Akasha OS :

> permettre aux modules d'héberger des applications riches sans ouvrir un WebView arbitraire.

Le runtime pourrait progressivement proposer des widgets spécialisés :

```text id="p69m71"
scene3d

timeline

node_graph

waveform

code_editor

data_grid

canvas
```

Ils restent :

```text id="65wrhv"
declarative
capability controlled
auditable
native
```

---

# 171. Declarative UI evolution

Le modèle actuel :

```text id="p5fj02"
Module
 ↓
Declarative Widget Tree
```

évolue vers :

```text id="lrr6s9"
Module
      │
      ▼
Declarative Widget Tree
      │
      ├── standard widgets
      │
      └── rich widgets
              │
              ├── scene3d
              ├── canvas
              ├── timeline
              └── ...
```

Les rich widgets restent des composants runtime approuvés.

---

# 172. Why not WebView

Un WebView donnerait énormément de liberté aux modules, mais introduirait :

```text id="5we4fw"
JavaScript runtime

DOM

browser APIs

network surface

dependency ecosystem

larger attack surface
```

Le widget natif `scene3d` conserve la philosophie :

```text id="tkj5gk"
Module declares intent

Akasha controls execution
```

plutôt que :

```text id="oqf3n7"
Module executes arbitrary UI code
```

---

# 173. ADR 1 — Rich Declarative Widgets

Créer une ADR Akasha :

```text id="7ysfkn"
ADR — Rich Declarative Widgets
```

Décision :

> Akasha declarative_ui peut exposer des widgets natifs complexes implémentés par le runtime, tout en conservant une API déclarative fermée pour les modules.

Premier widget de référence :

```text id="j29i9l"
scene3d
```

---

# 174. ADR 2 — SceneGraph

Créer :

```text id="wbdwm4"
ADR — Canonical SceneGraph
```

Décision :

> Les applications 3D Akasha utilisent un SceneGraph indépendant du moteur de rendu.

Le format ne doit dépendre ni de :

```text id="7krqgl"
Blender
Godot
Unity
Unreal
```

---

# 175. ADR 3 — External Render Backends

Créer :

```text id="ohhiyv"
ADR — Sandboxed Render Backends
```

Décision :

> Les moteurs de rendu externes sont utilisés via `RenderService` et ne reçoivent aucune capability implicite du module appelant.

---

# 176. ADR 4 — Asset Identity

Créer :

```text id="ftlh82"
ADR — Content Addressed Assets
```

Les assets devraient idéalement être identifiés par contenu.

Exemple :

```text id="c4vhqk"
sha256
```

permettant :

```text id="hchwnl"
deduplication
integrity
reproducibility
cache
```

---

# 177. ADR 5 — Scene Operations

Créer :

```text id="5qjjsm"
ADR — Transactional Scene Operations
```

Toutes les modifications sont exprimées sous forme d'opérations.

Cela fournit nativement :

```text id="ouh0pp"
undo
redo
audit
agent trace
transactions
collaboration
```

---

# 178. Suggested Akasha repository structure

Une organisation possible :

```text id="uqmij5"
akasha-os/

crates/

  aos-scene/
      scene graph
      transforms
      serialization
      operations

  aos-scene-runtime/
      runtime
      selection
      hierarchy
      cameras
      lights

  aos-rig/
      skeletons
      FK
      IK
      constraints

  aos-assets/
      asset registry
      loading
      cache

  aos-render/
      render API
      jobs
      backend abstraction

  aos-render-blender/
      blender adapter
      blender process isolation

  aos-ui/
      ...
      scene3d widget

modules/

  illustration-studio/
```

Les noms exacts doivent évidemment être adaptés à l'organisation réelle du workspace.

---

# 179. `aos-scene`

Cette crate doit rester légère.

Responsabilités :

```text id="s32kec"
SceneGraph
Node
Transform
Camera
Light
AssetRef
metadata
serialization
validation
operations
```

Elle ne dépend idéalement pas du renderer.

---

# 180. `aos-scene-runtime`

Responsabilités :

```text id="2ss84a"
runtime hierarchy
world transforms
selection
visibility
scene lifecycle
spatial queries
```

---

# 181. `aos-rig`

Responsabilités :

```text id="aqpku8"
Skeleton
Bone
JointConstraint

FK

IK solver

Pose

PosePreset

LookAtConstraint
```

Elle doit fonctionner indépendamment du mesh.

---

# 182. `aos-assets`

Responsabilités :

```text id="0skvh5"
AssetRegistry

AssetMetadata

AssetResolver

AssetCache

LOD

dependency resolution
```

---

# 183. `aos-render`

Définit uniquement l'abstraction.

Par exemple :

```rust id="js3j2m"
trait RenderBackend {
    fn capabilities(&self) -> RenderCapabilities;

    fn submit(
        &self,
        request: RenderRequest
    ) -> RenderJob;

    fn cancel(
        &self,
        job: RenderJobId
    );
}
```

Le module ne dépend jamais directement de Blender.

---

# 184. `aos-render-blender`

Implémente :

```text id="utmvjs"
SceneGraph
    ↓
Blender representation
    ↓
Render
    ↓
Render passes
```

Cette crate/service peut évoluer indépendamment.

---

# 185. SceneGraph Rust model

Conceptuellement :

```rust id="8avh7e"
struct Scene {
    id: SceneId,
    nodes: Vec<Node>,
    active_camera: Option<NodeId>,
    metadata: SceneMetadata,
}
```

Node :

```rust id="awqbmz"
struct Node {
    id: NodeId,
    name: String,
    parent: Option<NodeId>,
    transform: Transform,
    kind: NodeKind,
}
```

---

# 186. NodeKind

```rust id="5uj6ye"
enum NodeKind {
    Group,
    Mesh(MeshNode),
    Character(CharacterNode),
    Camera(CameraNode),
    Light(LightNode),
    Empty,
}
```

Cette enum pourra être étendue.

---

# 187. Asset references

Ne jamais stocker un chemin système brut comme identité principale.

Préférer :

```rust id="k9c8xq"
struct AssetRef {
    id: AssetId,
    version: AssetVersion,
}
```

Le registry résout ensuite :

```text id="j7s1tr"
AssetRef
 ↓
authorized local path
```

---

# 188. Scene operations

Exemple :

```rust id="fd7r8o"
enum SceneOperation {

    AddNode(...),

    RemoveNode(...),

    SetTransform(...),

    SetProperty(...),

    SetPose(...),

    SetActiveCamera(...),

}
```

Chaque opération reçoit :

```text id="k1vmzq"
operation id
actor
timestamp
scene revision
```

---

# 189. Actor

Un acteur peut être :

```text id="uxcfu6"
Human

Agent

Module

System
```

Exemple :

```yaml id="uug4pg"
actor:

  type: agent

  id: illustration-director
```

Cela rend l'audit extrêmement clair.

---

# 190. Scene revision

Chaque transaction validée incrémente :

```text id="69hyka"
scene_revision
```

Exemple :

```text id="r1gy7l"
41
 ↓
transaction
 ↓
42
```

Les render jobs référencent explicitement :

```text id="y6ljwp"
revision 42
```

---

# 191. Scene diff

Grâce aux opérations :

```text id="jgvb6w"
scene.diff(40, 42)
```

peut produire :

```text id="ysbhd9"
Man:
  head rotation changed

Camera:
  position changed

Light01:
  intensity changed
```

Très utile pour expliquer les actions de l'agent.

---

# 192. Agent tool authorization

Chaque tool doit correspondre à une capability.

Exemple :

```text id="v2j7z8"
scene.read
```

autorise :

```text id="a2jbr8"
scene.inspect
```

mais pas :

```text id="9x18mk"
scene.node.update
```

Pour modifier :

```text id="95g1qy"
scene.write
```

---

# 193. Fine-grained capabilities

Une évolution peut autoriser :

```text id="5q3btj"
scene.camera.write

scene.pose.write

scene.light.write

scene.assets.write
```

Ainsi un Camera Agent ne peut réellement modifier que les caméras.

---

# 194. Render capability

Exemple :

```text id="jzrxjq"
render.preview
```

et :

```text id="bxv9m3"
render.high_cost
```

peuvent être distinctes.

Cela permet à Akasha de demander une validation avant un rendu coûteux.

---

# 195. Resource governance

Un render job doit déclarer ses besoins.

Exemple :

```yaml id="9g5urh"
resources:

  gpu: preferred

  vram_mb: 4096

  cpu_threads: 8

  estimated_duration: 20s
```

Akasha décide :

```text id="t2d0l7"
accept
queue
degrade
deny
```

---

# 196. Multi-machine future

Cette abstraction est compatible avec le cluster Akasha.

Aujourd'hui :

```text id="e4mqvq"
RenderService
 ↓
local GPU
```

Demain :

```text id="28vb8x"
RenderService
      │
      ├── Local GPU
      │
      ├── Workstation GPU
      │
      └── LAN render node
```

Le module ne change pas.

---

# 197. Asset distribution future

Même principe :

```text id="knfl4j"
AssetRef
```

peut être résolu :

```text id="yvl5g5"
local cache

LAN asset store

community repository

authorized remote source
```

sans modifier le SceneGraph.

---

# 198. Privacy

Un projet d'illustration peut rester entièrement local.

Mode :

```text id="2sywjc"
Offline
```

garantit :

```text id="9ak5fd"
no external asset search

no cloud LLM

no remote rendering

no telemetry containing scene data
```

---

# 199. Trust boundary

Architecture de confiance :

```text id="7cl1fh"
                     AKASHA

Module
  │
  ▼
Capability Layer
  │
  ├──── Scene Service
  │
  ├──── Asset Service
  │
  └──── Render Service
              │
              ▼
       Sandbox Boundary
              │
              ▼
           Blender
```

Blender ne reçoit jamais directement les credentials ou capabilities Akasha.

---

# 200. Main technical risks

## Risk 1 — `scene3d`

C'est le risque principal.

Le système UI actuel doit pouvoir héberger un viewport GPU riche sans compromettre :

```text id="nsednp"
security
portability
declarative model
performance
```

---

# 201. Risk 2 — Cross-platform rendering

Akasha cible plusieurs environnements.

Il faudra gérer :

```text id="7sjkgx"
Windows

Linux

macOS Apple Silicon
```

Blender simplifie une partie du problème mais augmente fortement la taille de distribution.

---

# 202. Risk 3 — Asset quality

Le résultat final dépend fortement de :

```text id="d1c64j"
asset consistency
rig quality
materials
scale
topology
```

Une bibliothèque incohérente donnera une scène incohérente.

Le MVP doit donc privilégier un **petit pack homogène**.

---

# 203. Risk 4 — Scene Planner hallucinations

Un LLM peut inventer :

```text id="93xqgr"
asset IDs
node IDs
unsupported relationships
invalid properties
```

Il ne doit donc jamais produire directement des commandes runtime non validées.

Pipeline obligatoire :

```text id="7s50e4"
LLM
 ↓
structured output
 ↓
schema validation
 ↓
semantic validation
 ↓
execution
```

---

# 204. Risk 5 — Pose generation

Transformer :

> L'homme pousse doucement la porte tout en regardant le chat.

en pose crédible est plus difficile qu'une simple extraction sémantique.

Le MVP doit donc combiner :

```text id="ps9j7r"
pose presets
+
constraints
+
IK
```

plutôt que demander au LLM de prédire toutes les rotations des os.

---

# 205. Risk 6 — NPR quality

Produire un simple contour toon est facile.

Produire un rendu réellement convaincant :

```text id="q55wy8"
pencil
watercolor
charcoal
marker
```

est beaucoup plus difficile.

Il faut considérer chaque style comme un mini-pipeline graphique à part entière.

---

# 206. Risk 7 — Scope explosion

Le projet peut très facilement devenir :

```text id="90cmj5"
Blender
+
Photoshop
+
Stable Diffusion
+
Mixamo
+
asset marketplace
```

Ce serait une erreur.

La première proposition de valeur doit rester :

> Transformer une idée en composition 3D éditable et obtenir une illustration contrôlable.

---

# 207. Recommended implementation order

Je recommande cet ordre précis :

```text id="4pc9qy"
1. scene3d spike

2. SceneGraph

3. viewport interaction

4. asset loading

5. humanoid rig

6. IK

7. project persistence

8. Blender adapter

9. basic line render

10. pencil NPR

11. Illustration Studio UI

12. SceneIntent

13. Scene Planner

14. Asset Resolver

15. Pose Planner

16. agent tools

17. composition assistant

18. additional styles
```

Ne surtout pas commencer par le LLM.

---

# 208. First technical milestone

Le premier milestone devrait être extrêmement simple :

```text id="0sxuej"
Akasha 3D Scene
```

avec :

```text id="pl65is"
floor

cube

rigged mannequin

camera

light
```

L'utilisateur peut :

```text id="10sc0k"
orbit camera

select mannequin

move mannequin

select hand target

move hand

render image
```

Si cela fonctionne proprement dans `declarative_ui`, le fondement architectural est validé.

---

# 209. Second milestone

```text id="i4pg8d"
NPR Proof of Concept
```

Scene :

```text id="cpq1on"
mannequin
chair
table
```

Sorties :

```text id="x2j1fo"
3D preview

Sketch

Pencil

Ink
```

Cela valide la chaîne :

```text id="kvq44l"
SceneGraph
 ↓
Blender
 ↓
Render passes
 ↓
NPR
```

---

# 210. Third milestone

```text id="cqsehk"
Prompt to Scene
```

Prompt :

> A man sits at a table.

Résultat attendu :

```text id="gghqzn"
room

table

chair

man
```

avec :

```text id="7s7l22"
man ON chair

man NEAR table

camera framing scene
```

---

# 211. Fourth milestone

Ajouter la scène cible :

> An old bookstore. A man enters through the door while a cat lies on the counter watching him.

C'est le premier véritable test du produit.


---

# 212. Definition of MVP Done

Le MVP est terminé lorsque l'utilisateur peut :

```text
describe
   ↓
compose
   ↓
pose
   ↓
adjust
   ↓
style
   ↓
render
   ↓
export
```

une illustration sans utiliser directement un logiciel 3D externe.

Concrètement, le workflow suivant doit fonctionner de bout en bout.

### 1. Describe

L'utilisateur saisit :

> An old bookstore. A man enters through the door while a cat lies on the counter watching him.

### 2. Generate

Akasha produit :

```text
SceneIntent
    ↓
SceneGraph
    ↓
Proxy Scene
    ↓
Resolved Assets
```

### 3. Compose

L'utilisateur peut déplacer :

```text
man
cat
counter
camera
lights
```

### 4. Pose

Il sélectionne l'homme.

Le rig devient visible.

Il peut modifier :

```text
head
hands
feet
pelvis
```

avec IK/FK.

### 5. Ask

Il demande :

> Make the man look at the cat and keep his left hand on the door.

L'agent modifie la pose sans toucher au reste de la composition.

### 6. Style

Il sélectionne :

```text
Pencil
```

### 7. Preview

Akasha génère une preview NPR.

### 8. Iterate

L'utilisateur demande :

> Use a lower camera angle.

La scène est modifiée.

### 9. Render

L'utilisateur lance :

```text
Final Render
```

### 10. Export

L'illustration est exportée en PNG.

La scène reste éditable.

C'est le critère fondamental.

---

# 213. Product invariant

Une règle doit rester vraie pendant toute l'évolution du produit :

> **L'illustration ne doit jamais devenir la seule source de vérité.**

La relation est toujours :

```text
Scene
  ↓
Render
  ↓
Illustration
```

et jamais :

```text
Illustration
  ↓
try to recover Scene
```

---

# 214. Second product invariant

Toute modification importante doit être représentable sous forme structurée.

Par exemple :

```text
"move the cat"
```

devient :

```json
{
  "operation": "SetTransform",
  "node": "cat",
  "position": [1.2, 0.8, 1.05]
}
```

et non une modification opaque du rendu.

---

# 215. Third product invariant

L'agent ne possède pas une version différente de la scène.

```text
Human
   │
   │
   ▼
SceneGraph
   ▲
   │
   │
Agent
```

Il n'existe qu'un état canonique.

---

# 216. Fourth product invariant

Le LLM ne manipule jamais directement le renderer.

Pipeline obligatoire :

```text
LLM
 ↓
Intent / Operations
 ↓
Validation
 ↓
Scene Runtime
 ↓
Render Service
```

Cela garantit :

```text
security
determinism
auditability
undo
validation
```

---

# 217. Fifth product invariant

Les moteurs externes sont des backends.

Blender doit être considéré comme :

```text
RenderBackend
```

et non comme :

```text
Illustration Studio Engine
```

Cette distinction évitera une dépendance architecturale difficile à supprimer plus tard.

---

# 218. Recommended MVP architecture

Architecture finale recommandée :

```text
┌────────────────────────────────────────────────────────────┐
│                        AKASHA OS                           │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │               Illustration Studio                    │  │
│  │                                                      │  │
│  │ Prompt                                               │  │
│  │   │                                                  │  │
│  │   ▼                                                  │  │
│  │ Scene Planner ───────► SceneIntent                   │  │
│  │                           │                          │  │
│  │                           ▼                          │  │
│  │                    Scene Composer                    │  │
│  │                           │                          │  │
│  │                 ┌─────────┼─────────┐                │  │
│  │                 ▼         ▼         ▼                │  │
│  │              Assets     Pose     Composition         │  │
│  │                 │         │         │                │  │
│  └─────────────────┼─────────┼─────────┼────────────────┘  │
│                    │         │         │                   │
│                    └─────────┼─────────┘                   │
│                              ▼                             │
│                       ┌────────────┐                       │
│                       │ SceneGraph │                       │
│                       └─────┬──────┘                       │
│                             │                              │
│              ┌──────────────┼──────────────┐               │
│              ▼              ▼              ▼               │
│          scene3d        Agent Tools    Persistence          │
│              │                                             │
│              ▼                                             │
│         Scene Runtime                                      │
│              │                                             │
│       ┌──────┼──────┐                                      │
│       ▼      ▼      ▼                                      │
│     GPU     Rig    Asset                                    │
│                     │                                      │
│                     ▼                                      │
│               Render Service                               │
│                     │                                      │
│                     ▼                                      │
│              Blender Backend                               │
│                     │                                      │
│                     ▼                                      │
│                  NPR                                       │
│                     │                                      │
│                     ▼                                      │
│              Illustration                                  │
└────────────────────────────────────────────────────────────┘
```

---

# 219. Component ownership

## Akasha Core

À intégrer au runtime :

```text
scene3d widget

SceneGraph

Scene Runtime

basic asset runtime

rig runtime

GPU viewport

RenderService abstraction

capabilities

scene operation log
```

---

## Render extension

Composant séparé :

```text
Blender Render Backend
```

Responsabilités :

```text
SceneGraph → Blender

render passes

Freestyle / Line Art

NPR processing

image output
```

---

## Illustration Studio

Module :

```text
project UI

SceneIntent

Scene Planner

Asset Resolver

Pose Planner

Composition Planner

illustration styles

agent tools

render orchestration
```

---

# 220. What should NOT enter Akasha Core

Éviter d'introduire dans le Core :

```text
pencil rendering

watercolor

illustration composition rules

bookstore generator

character style presets

illustration-specific prompts
```

Ce sont des fonctionnalités du module ou de packages spécialisés.

---

# 221. Generic vs domain-specific rule

Une règle simple permet de décider où mettre une fonctionnalité.

Question :

> Une application non liée à l'illustration pourrait-elle raisonnablement utiliser cette fonctionnalité ?

Si oui :

```text
Akasha runtime
```

Exemples :

```text
SceneGraph
camera
rig
IK
asset loading
scene3d
render jobs
```

Si non :

```text
Illustration Studio
```

Exemples :

```text
pencil style
composition assistant
illustration presets
watercolor
storyboard layout
```

---

# 222. Recommended repository plan

Je découperais le travail dans Akasha OS en **un epic et plusieurs issues**, plutôt qu'une seule énorme issue.

Epic :

```text
Epic — Rich 3D Applications & Illustration Studio
```

Puis les issues suivantes.

---

# 223. Issue 1

## Add native `scene3d` declarative widget

Objectif :

Permettre à un module d'afficher une scène 3D native sans WebView.

Scope :

```text
viewport

camera orbit

pan

zoom

node selection

transform gizmos

declarative events
```

Pas encore :

```text
AI
NPR
Blender
```

---

# 224. Issue 2

## Introduce canonical SceneGraph

Créer :

```text
Scene
Node
Transform
Mesh
Camera
Light
Group
AssetRef
```

Avec :

```text
serialization
validation
operations
revisioning
```

---

# 225. Issue 3

## Add Scene Runtime

Implémenter :

```text
hierarchy

world transforms

visibility

selection

camera management

spatial queries
```

---

# 226. Issue 4

## Add generic rigging runtime

Implémenter :

```text
Skeleton

Bone

Pose

FK

IK

LookAt

JointConstraint
```

Premier profil :

```text
humanoid
```

Deuxième :

```text
quadruped
```

---

# 227. Issue 5

## Add 3D asset registry

Implémenter :

```text
AssetRef

AssetMetadata

AssetRegistry

local cache

LOD

hash validation
```

---

# 228. Issue 6

## Introduce RenderService

Créer l'abstraction :

```text
render.submit

render.status

render.cancel

render.result
```

avec :

```text
resource declaration

job queue

scene revision binding

capabilities
```

---

# 229. Issue 7

## Add sandboxed Blender render backend

Implémenter :

```text
SceneGraph adapter

Blender scene creation

camera

lights

meshes

characters

render passes

isolated execution
```

---

# 230. Issue 8

## NPR proof of concept

Produire trois rendus :

```text
Sketch

Pencil

Ink
```

depuis la même scène.

Tester :

```text
line extraction

depth

normal

object IDs

paper textures

hatching
```

---

# 231. Issue 9

## Create Illustration Studio module shell

Créer le module :

```text
illustration-studio
```

avec les modes :

```text
Compose

Pose

Camera

Lighting

Style

Render
```

Sans IA dans un premier temps.

---

# 232. Issue 10

## Add SceneIntent schema

Définir :

```text
environment

subjects

objects

actions

relationships

constraints

composition hints
```

et son JSON Schema.

---

# 233. Issue 11

## Implement Prompt → SceneIntent

Ajouter l'interprétation LLM structurée.

Pipeline :

```text
Prompt

 ↓

LLM

 ↓

SceneIntent candidate

 ↓

Schema validation

 ↓

Semantic validation

 ↓

SceneIntent
```

---

# 234. Issue 12

## Implement Scene Composer

Transformer :

```text
SceneIntent
```

en :

```text
SceneGraph
```

d'abord avec proxy geometry.

---

# 235. Issue 13

## Implement semantic Asset Resolver

Pipeline :

```text
SceneIntent object

 ↓

semantic query

 ↓

Asset Registry

 ↓

ranking

 ↓

AssetRef
```

---

# 236. Issue 14

## Implement Pose Planner

Transformer :

```text
action semantics
```

en :

```text
pose preset

+

IK constraints

+

look-at constraints
```

---

# 237. Issue 15

## Implement Composition Planner

Déterminer :

```text
camera

lens

subject framing

visual hierarchy
```

à partir du SceneIntent.

---

# 238. Issue 16

## Expose Illustration Studio agent tools

Ajouter :

```text
scene.*

asset.*

rig.*

camera.*

light.*

composition.*

render.*
```

avec capabilities adaptées.

---

# 239. Issue 17

## Add transactional scene editing

Implémenter :

```text
BEGIN

operations

validation

COMMIT / ROLLBACK
```

pour les modifications agentiques.

---

# 240. Issue 18

## Add illustration style packages

Définir le format :

```text
StylePack
```

et fournir :

```text
Sketch

Pencil

Ink
```

comme premier pack.

---

# 241. Suggested epic dependency graph

```text
                    SceneGraph
                       │
            ┌──────────┴──────────┐
            ▼                     ▼
      Scene Runtime          Asset Registry
            │
            ▼
         scene3d
            │
            ▼
        Rig Runtime
            │
            ├──────────────────┐
            │                  │
            ▼                  ▼
   Illustration UI       RenderService
            │                  │
            │                  ▼
            │          Blender Backend
            │                  │
            │                  ▼
            │                NPR
            │
            ▼
       SceneIntent
            │
      ┌─────┼──────┐
      ▼     ▼      ▼
   Assets  Pose  Composition
      └─────┼──────┘
            ▼
       Agent Tools
```

---

# 242. Development streams

Une fois `SceneGraph` stabilisé, plusieurs travaux peuvent avancer en parallèle.

### Stream A — Runtime

```text
scene3d
rig
viewport
```

### Stream B — Rendering

```text
RenderService
Blender
NPR
```

### Stream C — Module

```text
Illustration Studio UI
projects
styles
```

### Stream D — AI

```text
SceneIntent
Scene Planner
Pose Planner
Composition
```

---

# 243. Tests

Il faut éviter de dépendre uniquement de tests visuels.

Plusieurs niveaux sont nécessaires.

---

# 244. SceneGraph unit tests

Tester :

```text
parenting

transform propagation

serialization

deserialization

node deletion

invalid references

revision increment

undo

redo
```

---

# 245. Rig tests

Tester :

```text
joint limits

FK

IK convergence

look-at

pose serialization

invalid skeleton

missing bones
```

---

# 246. SceneIntent tests

Dataset de prompts déterministes.

Exemple :

```text
"A cat sits on a table."
```

doit produire au minimum :

```text
cat

table

ON(cat, table)
```

---

# 247. Spatial tests

Exemple :

```text
"A man stands behind a chair."
```

doit conserver :

```text
BEHIND(man, chair)
```

jusqu'au SceneGraph.

---

# 248. Render regression tests

Créer quelques scènes de référence.

```text
cube

mannequin

room

bookstore
```

Générer les passes et comparer :

```text
geometry

depth

line

object ID
```

avec tolérance.

Les styles artistiques nécessiteront des tests plus souples.

---

# 249. End-to-end tests

Test principal :

```text
Prompt

→ SceneIntent

→ SceneGraph

→ Asset Resolution

→ Pose

→ Camera

→ Render

→ PNG
```

---

# 250. Security tests

Vérifier notamment :

```text
module cannot access arbitrary files

renderer cannot access vault

renderer cannot initiate network

asset paths cannot escape registry

malformed SceneGraph rejected

unauthorized scene operations rejected
```

---

# 251. Performance benchmarks

Créer un benchmark standard.

### Small

```text
1 character
20 objects
```

### Medium

```text
3 characters
100 objects
```

### Large

```text
10 characters
500 objects
```

Mesurer :

```text
load time

FPS

VRAM

IK latency

serialization

preview rendering

final rendering
```

---

# 252. Observability

Akasha doit pouvoir afficher :

```text
Scene

124 nodes

3 characters

246k triangles

VRAM: 1.8 GB

Viewport: 58 FPS

Render backend: Blender

Render job: 73%
```

Cela rejoint naturellement les métriques hardware déjà présentes dans Akasha OS.

---

# 253. Diagnostics

Une commande/module diagnostic pourrait retourner :

```text
scene3d runtime       OK

GPU                   OK

Asset registry        OK

Rig solver            OK

RenderService         OK

Blender backend       OK

NPR styles            3 installed
```

---

# 254. Graceful degradation

Si Blender n'est pas installé/disponible :

```text
3D editing
```

doit continuer à fonctionner.

Seul :

```text
final NPR rendering
```

est indisponible.

Même principe pour l'IA.

Si aucun modèle n'est disponible :

```text
manual scene creation
```

reste possible.

---

# 255. Feature capability matrix

| Feature | CPU | GPU | LLM | Blender |
|---|---|---|---|---|
| Scene editing | ✓ | optional | — | — |
| Character posing | ✓ | optional | — | — |
| Prompt parsing | ✓ | — | ✓ | — |
| Asset search | ✓ | — | optional | — |
| NPR preview | ✓ | preferred | — | ✓ |
| Final render | ✓ | preferred | — | ✓ |
| Manual project editing | ✓ | optional | — | — |

Le module ne doit donc pas devenir inutilisable lorsqu'une brique optionnelle manque.

---

# 256. Recommended first release boundary

Je ne mettrais pas immédiatement Illustration Studio dans la distribution Akasha principale.

Première étape :

```text
Experimental module
```

nécessitant :

```text
scene3d experimental feature
```

Cela permet d'éprouver le système de rich widgets avant d'en faire une API stable.

---

# 257. Runtime API stability

Marquer initialement :

```text
scene3d API: experimental
```

puis :

```text
scene3d v0
```

après le prototype.

Ne stabiliser :

```text
scene3d v1
```

qu'après au moins deux consommateurs.

Idéalement :

```text
Illustration Studio

+

second 3D module
```

Cela évite de concevoir une API trop spécifique au premier cas d'usage.

---

# 258. Suggested second consumer

Un excellent second module de validation serait :

```text
Scene Viewer
```

Module extrêmement simple permettant :

```text
load GLTF

inspect hierarchy

move camera

select objects
```

Si `scene3d` fonctionne aussi bien pour :

```text
Illustration Studio
```

que :

```text
Scene Viewer
```

l'abstraction est probablement suffisamment générique.


---

# 259. Supported 3D format

Pour les assets 3D, le format privilégié doit être :

```text id="9uy9p6"
glTF 2.0
```

et notamment sa représentation binaire :

```text id="eg8e06"
GLB
```

Raisons :

```text id="l6v9xr"
open standard

compact

widely supported

meshes

materials

textures

skeletons

animations

custom metadata
```

Le format projet Akasha reste néanmoins le `SceneGraph`.

---

# 260. SceneGraph vs GLTF

Il ne faut pas confondre :

```text id="g73v57"
SceneGraph
```

et :

```text id="xkch60"
GLTF
```

Le SceneGraph contient la sémantique Akasha :

```text id="kv4vwe"
characters

relationships

locks

asset references

agent metadata

history

scene revisions
```

GLTF contient principalement la représentation graphique.

La relation est :

```text id="b7zcs8"
SceneGraph
   │
   ├── AssetRef → GLB
   ├── AssetRef → GLB
   └── AssetRef → GLB
```

---

# 261. Asset package

Un asset Akasha pourrait être :

```text id="k8kx5i"
assets/
└── cat/
    ├── asset.yaml
    ├── model.glb
    ├── preview.webp
    └── license.txt
```

Pour un asset riggé :

```text id="4ez8ol"
cat/
├── asset.yaml
├── model.glb
├── rig.yaml
├── poses/
│   ├── standing.pose
│   ├── sitting.pose
│   └── lying.pose
└── preview.webp
```

---

# 262. Asset manifest

Exemple :

```yaml id="h01v5e"
id: animal.cat.generic

version: 1.0.0

type: character

format: glb

model: model.glb

tags:
  - cat
  - animal
  - domestic
  - quadruped

rig:
  profile: quadruped.cat
  file: rig.yaml

dimensions:
  height: 0.28
  length: 0.48

lod:
  supported: true

license:
  type: CC0

hash:
  sha256: "..."
```

---

# 263. Material model

Pour le MVP, utiliser un sous-ensemble simple du modèle PBR.

```text id="jox9hq"
base color

roughness

metallic

normal

alpha
```

Les styles NPR ne doivent pas dépendre directement de matériaux Blender spécifiques.

---

# 264. Semantic materials

Ajouter une couche sémantique optionnelle :

```yaml id="bafpnx"
material:

  semantic_type: wood

  properties:
    age: old
    finish: matte
```

Cela permet au style NPR d'adapter son traitement.

Par exemple :

```text id="aw4p8c"
wood
```

peut recevoir davantage de hachures qu'une surface :

```text id="8vdyam"
glass
```

---

# 265. Material classes

Classes utiles :

```text id="c8e4nv"
skin

hair

fabric

wood

metal

glass

stone

paper

vegetation

water
```

Ces informations peuvent également aider les futurs styles.

---

# 266. Character rig standard

Il faut définir un standard Akasha indépendant des noms de bones propres aux assets.

Exemple :

```text id="wt68ym"
humanoid.hips

humanoid.spine

humanoid.chest

humanoid.neck

humanoid.head

humanoid.arm.left.upper

humanoid.arm.left.lower

humanoid.hand.left
```

Le fichier `rig.yaml` mappe le modèle vers ce standard.

---

# 267. Rig mapping

Exemple :

```yaml id="qcz14h"
profile: humanoid.v1

mapping:

  humanoid.hips:
    bone: mixamorig:Hips

  humanoid.spine:
    bone: mixamorig:Spine

  humanoid.head:
    bone: mixamorig:Head

  humanoid.hand.left:
    bone: mixamorig:LeftHand
```

Cela permet d'utiliser des personnages provenant de sources différentes.

---

# 268. Retargeting

Grâce au standard :

```text id="iz9yve"
PosePreset
      │
      ▼
HumanoidRig
      │
      ├── Character A
      ├── Character B
      └── Character C
```

Les poses deviennent réutilisables.

---

# 269. Pose format

Exemple :

```yaml id="kx02r5"
id: entering-door

profile: humanoid.v1

joints:

  humanoid.hips:
    rotation: [...]

  humanoid.spine:
    rotation: [...]

constraints:

  - type: hand_target
    hand: left
    semantic_target: door.handle

  - type: look_at
    target: scene.subject.cat
```

Les contraintes sémantiques peuvent être résolues au moment de l'application.

---

# 270. Interaction anchors

Les assets importants peuvent exposer des points d'interaction.

Une porte :

```yaml id="u1g01g"
anchors:

  - id: handle
    type: hand_grip

  - id: hinge
    type: rotation_axis
```

Une chaise :

```yaml id="34qqmi"
anchors:

  - id: seat
    type: sit_target
```

Un comptoir :

```yaml id="iqshs7"
anchors:

  - id: surface
    type: placement_surface
```

---

# 271. Why interaction anchors matter

Sans anchors, l'agent doit deviner la géométrie.

Avec :

```text id="l8g5mg"
door.handle
```

il peut demander :

```text id="3pkbgz"
rig.attach_hand(
    character = man,
    hand = left,
    target = door.handle
)
```

Le système calcule ensuite la position réelle.

---

# 272. Semantic asset affordances

Un asset peut déclarer ce qu'il permet.

Exemple :

```yaml id="w3jck1"
affordances:

  - openable

  - hand_grippable

  - walk_through
```

Une chaise :

```yaml id="vth31c"
affordances:

  - sittable
```

Un comptoir :

```yaml id="t5b2u5"
affordances:

  - support_surface
```

---

# 273. Scene semantics

On obtient alors une scène qui n'est plus seulement géométrique.

```text id="4bdc9f"
Door
 ├── geometry
 ├── transform
 ├── handle
 └── affordance: openable

Counter
 ├── geometry
 └── affordance: support_surface

Cat
 ├── geometry
 ├── rig
 └── semantic type: animal
```

C'est particulièrement intéressant pour Akasha, car les agents peuvent raisonner sur le monde sans analyser les triangles.

---

# 274. Semantic scene layer

Architecture :

```text id="19czbp"
Semantic Scene
      │
      ▼
SceneGraph
      │
      ▼
Geometry
```

Le `SceneIntent` construit d'abord la couche sémantique.

Le Scene Composer la matérialise ensuite.

---

# 275. Example semantic query

L'agent peut demander :

```text id="1h4njm"
scene.query(
  type = "support_surface",
  near = "cat"
)
```

et recevoir :

```text id="o78oxq"
counter.surface
```

---

# 276. Semantic actions

Cela ouvre des commandes de haut niveau :

```text id="y3etf2"
character.sit_on(chair)

character.look_at(cat)

character.hold(book)

character.open(door)

object.place_on(counter)
```

Ces actions sont converties en contraintes géométriques.

---

# 277. High-level action planner

Architecture :

```text id="fddzn7"
"Man opens the door"
       │
       ▼
Semantic Action
       │
       ▼
OPEN(man, door)
       │
       ▼
Constraints
       │
       ├── hand → handle
       ├── body → near door
       ├── facing → door
       └── door rotation
       │
       ▼
IK + Scene operations
```

C'est nettement plus robuste que demander au LLM des coordonnées 3D.

---

# 278. Scene Planner rule

Principe important :

> Le LLM doit produire des intentions spatiales et sémantiques, jamais des coordonnées précises lorsqu'un solveur peut les calculer.

Préférer :

```text id="v7jowm"
ON(cat, counter)
```

à :

```text id="dhscq3"
cat.position = [1.342, 0.728, 1.041]
```

---

# 279. Deterministic solvers

Les coordonnées sont déterminées par :

```text id="yexhm8"
layout solver

constraint solver

IK solver

collision queries

bounding boxes
```

Le LLM reste responsable du :

```text id="1uj9j1"
what
```

et les solveurs du :

```text id="z5ks7p"
how
```

---

# 280. Composition solver

Même logique pour la caméra.

LLM :

```text id="p48x31"
medium wide shot

cat primary subject

man secondary subject

slightly low angle
```

Solver :

```text id="7yyeyq"
camera.position

camera.rotation

focal_length
```

---

# 281. Hybrid deterministic/AI architecture

C'est probablement le modèle le plus important du projet :

```text id="gcvhfa"
              LLM

               │
        semantic intent
               │
               ▼
        deterministic
           systems
               │
      ┌────────┼────────┐
      ▼        ▼        ▼
    Layout     IK     Camera
    Solver   Solver    Solver
      │        │        │
      └────────┼────────┘
               ▼
           SceneGraph
```

Cela correspond bien à l'approche agentique contrôlée d'Akasha OS.

---

# 282. Suggested technology choices

Pour le premier prototype :

### Core

```text id="n3sqcw"
Rust
```

pour :

```text id="ssib11"
SceneGraph

Scene Runtime

Rig

Capabilities

IPC

RenderService
```

Cela reste cohérent avec Akasha OS.

---

# 283. Viewport renderer

Deux options principales.

### Option A — `wgpu`

Utiliser :

```text id="s95x5p"
wgpu
```

pour le viewport natif.

Avantages :

```text id="wnn6kp"
Rust-native

cross-platform

Vulkan

DirectX 12

Metal

WebGPU architecture
```

Cela paraît être le choix naturel pour Akasha.

---

# 284. Option B — existing engine

Utiliser un moteur tel que :

```text id="b1vcjv"
Bevy
```

comme couche de scène.

Avantage :

```text id="h02g8v"
faster prototype
```

Inconvénient :

```text id="7ovl9c"
large dependency

engine architecture constraints

potential duplication with Akasha runtime
```

---

# 285. Recommendation

Pour un spike :

```text id="n6hs7c"
wgpu
+
small scene runtime
```

me paraît préférable.

Éviter de faire de Bevy une dépendance structurante tant que les besoins précis ne sont pas connus.

---

# 286. Math

Utiliser une bibliothèque Rust dédiée pour :

```text id="7u9akx"
Vec3

Quat

Mat4

Transform
```

Le format sérialisé reste indépendant de cette bibliothèque.

---

# 287. Picking

Le viewport doit supporter la sélection GPU.

Technique possible :

```text id="ct7sgb"
ID buffer
```

Chaque node reçoit un identifiant de picking.

```text id="6au3uv"
mouse click

 ↓

pixel object ID

 ↓

SceneNode
```

---

# 288. Transform gizmos

Le runtime doit fournir nativement :

```text id="8e2v1j"
translate gizmo

rotate gizmo

scale gizmo
```

Ils ne sont pas implémentés par chaque module.

---

# 289. Bone picking

Même système pour les rigs.

```text id="klggml"
bone ID buffer
```

ou primitives de picking dédiées.

L'utilisateur sélectionne :

```text id="pbqzdr"
hand

elbow

head

IK target
```

---

# 290. Blender integration strategy

Éviter de piloter Blender par interface graphique.

Utiliser :

```text id="mqb7dp"
Blender headless
```

avec un adapter contrôlé.

Concept :

```text id="dl7rrn"
Akasha

 ↓

RenderPackage

 ↓

Blender Adapter

 ↓

temporary .blend / runtime scene

 ↓

blender --background

 ↓

render outputs
```

---

# 291. RenderPackage

Le renderer reçoit un package immuable.

```text id="gyk7zk"
render-job/
│
├── scene.json
├── render.json
├── assets/
└── output/
```

Le package correspond exactement à une révision.

---

# 292. Blender adapter

Le script d'adaptation doit :

```text id="9p8t8p"
load assets

apply transforms

create cameras

create lights

apply skeleton poses

configure render engine

configure NPR

configure compositor

render passes
```

---

# 293. Blender isolation

Le process doit fonctionner dans une sandbox avec :

```text id="xd7fj7"
network denied

filesystem restricted

environment sanitized

time limit

memory limit

output limit
```

---

# 294. Blender version

Le backend doit annoncer sa version.

```text id="thvvqy"
backend:
  id: blender-npr

  version: 1.0

  blender:
    minimum: ...
```

Akasha ne doit pas dépendre silencieusement de la version installée par l'utilisateur.

---

# 295. Bundled vs external Blender

Deux stratégies possibles.

### External

Utiliser Blender installé.

Avantages :

```text id="csn8bi"
small Akasha package
```

Inconvénients :

```text id="ajy4j8"
version mismatch

configuration differences
```

### Managed runtime

Akasha télécharge une version connue.

Avantages :

```text id="s4m0g1"
reproducible

controlled
```

Inconvénient :

```text id="6lsk5a"
large download
```

---

# 296. Recommendation

Utiliser le modèle déjà présent dans Akasha pour les packs/modèles :

```text id="b7w7h8"
optional managed renderer pack
```

Exemple :

```text id="27vg0q"
Illustration Renderer Pack
```

contenant :

```text id="2o1fng"
Blender runtime

adapter

NPR scripts

default textures

style processors
```

Le module reste léger.

---

# 297. Renderer packs

Cela peut devenir une abstraction Akasha générale :

```text id="l9kltu"
Render Backend Pack
```

Exemples futurs :

```text id="t1wdqv"
Blender NPR

Cycles High Quality

Native NPR

Godot Renderer
```

---

# 298. Default styles package

Premier package :

```text id="igxyn8"
illustration-basic
```

avec :

```text id="lfas02"
Sketch

Pencil

Ink
```

---

# 299. Pencil technical prototype

Première implémentation :

```text id="m2ckxq"
Freestyle / Line Art
+
ambient occlusion
+
grayscale diffuse
+
procedural hatch
+
paper texture
+
slight line noise
```

Ne pas chercher immédiatement à simuler physiquement le graphite.

---

# 300. Ink technical prototype

Pipeline :

```text id="rvnyjf"
Line Art

+

variable width

+

black fill threshold

+

minimal hatching

+

paper
```

---

# 301. Sketch technical prototype

Pipeline :

```text id="5v1uk8"
Line Art

+

multiple offset strokes

+

high jitter

+

construction-like light lines

+

minimal shading
```

Ce style peut volontairement conserver une apparence de dessin préparatoire.

---

# 302. Watercolor later

L'aquarelle nécessite davantage :

```text id="oz0ksv"
pigment diffusion

edge darkening

granulation

paper interaction

color bleeding

wet/dry behavior
```

Il est préférable de la traiter comme une feature V1/V2 séparée.

---

# 303. Future neural NPR

Une évolution possible :

```text id="11ck46"
Geometry Passes

 ↓

small local image model

 ↓

stylized refinement
```

Cela pourrait améliorer :

```text id="y6qnv5"
watercolor

gouache

pastel

storybook
```

tout en conservant la structure 3D.

---

# 304. Neural renderer constraint

Le modèle reçoit :

```text id="1pq0jj"
RGB base

depth

normal

line art

object masks

style
```

et doit conserver la structure.

Une variation trop importante doit pouvoir être détectée.

---

# 305. Structural validation

Après un neural render :

```text id="f2k8j5"
render

 ↓

structure comparison

 ↓

depth/edge consistency

 ↓

accept / warn
```

Cette étape évite qu'un modèle ajoute ou supprime arbitrairement des éléments importants.


---

# 306. No neural dependency

Point important :

> Illustration Studio doit être utile même sans modèle de génération d'image.

Le pipeline fondamental reste :

```text id="77s74x"
Prompt
  ↓
SceneIntent
  ↓
SceneGraph
  ↓
3D composition
  ↓
NPR renderer
  ↓
Illustration
```

Un modèle de génération d'image peut éventuellement devenir un post-process optionnel, mais ne doit jamais être indispensable au fonctionnement du module.

---

# 307. Final MVP architecture

Le MVP doit rester volontairement limité à cinq grandes briques.

```text id="c6qqzg"
┌──────────────────────────────────────────────────┐
│                Illustration Studio               │
│                                                  │
│ Prompt ────────► Scene Planner                   │
│                      │                           │
│                      ▼                           │
│                 SceneIntent                      │
│                      │                           │
│                      ▼                           │
│                Scene Composer                    │
└──────────────────────┬───────────────────────────┘
                       │
                       ▼
                 ┌───────────┐
                 │SceneGraph │
                 └─────┬─────┘
                       │
           ┌───────────┼─────────────┐
           ▼           ▼             ▼
        scene3d       Rig          Assets
           │
           ▼
      Scene Runtime
           │
           ▼
     Render Service
           │
           ▼
    Blender NPR Pack
           │
           ▼
      Illustration
```

---

# 308. MVP technology stack

### Akasha Runtime

```text id="jtx4yv"
Rust
```

### 3D viewport

```text id="2q09qk"
wgpu
```

### Asset format

```text id="w1ttfv"
GLB / glTF 2.0
```

### Scene format

```text id="3k05ba"
Akasha SceneGraph
```

sérialisable en JSON/YAML.

### Rig

```text id="3tt43b"
Akasha normalized rig
+
FK
+
IK
```

### Renderer

```text id="uv7yp4"
Blender headless
```

### NPR

```text id="bj00e6"
Line Art / Freestyle
+
render passes
+
compositor
```

### AI

LLM local ou provider Akasha existant avec structured output.

---

# 309. MVP styles

Uniquement :

```text id="6sv5aq"
Sketch

Pencil

Ink
```

Ils doivent être suffisamment différents pour valider l'abstraction `Style`.

Ne pas inclure l'aquarelle dans le MVP.

---

# 310. MVP assets

Créer un petit pack cohérent d'environ :

```text id="75bxvg"
3 humans

2 animals

10 furniture assets

10 architectural assets

20 props
```

Soit environ :

```text id="3eq1pr"
45 assets
```

Il vaut mieux 45 assets homogènes que plusieurs milliers d'assets disparates.

---

# 311. MVP semantic capabilities

Supporter uniquement :

```text id="40qrx9"
ON

INSIDE

NEAR

BEHIND

IN_FRONT_OF

FACING

LOOKING_AT

HOLDING
```

Cela suffit déjà à couvrir énormément de scènes simples.

---

# 312. MVP semantic actions

Supporter :

```text id="pajsvf"
stand

sit

lie

walk

look_at

hold

open
```

Le Scene Planner mappe les actions du prompt vers ces primitives.

---

# 313. MVP character interactions

Supporter :

```text id="e7dp4j"
hand → object

foot → ground

character → chair

character → look target

object → support surface
```

Le reste viendra ultérieurement.

---

# 314. MVP UI

Six modes seulement :

```text id="b1t5qo"
Compose

Pose

Camera

Light

Style

Render
```

Éviter une interface ressemblant à Blender.

---

# 315. Compose

Outils :

```text id="2qvtgq"
Select

Move

Rotate

Scale

Delete

Duplicate
```

---

# 316. Pose

Outils :

```text id="1w3st4"
Skeleton

IK handles

FK rotation

Look target

Pose presets

Reset pose
```

---

# 317. Camera

Outils :

```text id="a3okmc"
Position

Rotation

Lens

Focus

Perspective / Orthographic

Composition guides
```

---

# 318. Light

Outils :

```text id="36thvb"
Add

Move

Rotate

Intensity

Temperature

Preset
```

---

# 319. Style

Interface :

```text id="jq5t63"
Sketch

Pencil

Ink

Line strength

Shading strength

Paper
```

---

# 320. Render

Interface :

```text id="dlrcfm"
Preview

Final

Resolution

Aspect ratio

Transparent background

Export
```

---

# 321. MVP agent commands

L'utilisateur doit pouvoir écrire naturellement :

> Move the cat closer to the edge.

> Make the man look at the cat.

> Put his hand on the door.

> Move the camera lower.

> Use a wider lens.

> Make the lighting warmer.

> Render this as pencil.

Ces commandes constituent le test fonctionnel principal de l'intégration agentique.

---

# 322. Agent execution model

Chaque demande suit :

```text id="b8dy8j"
Natural language

      ↓

Agent reasoning

      ↓

Semantic operations

      ↓

Validation

      ↓

Scene transaction

      ↓

SceneGraph revision
```

Jamais :

```text id="b3icod"
Natural language

      ↓

arbitrary code
```

---

# 323. Undo guarantee

Toute modification agentique doit être annulable par :

```text id="es0gmh"
Ctrl + Z
```

exactement comme une modification humaine.

C'est un principe UX important :

> L'agent est un autre opérateur de l'application, pas une autorité extérieure à son modèle d'édition.

---

# 324. Project persistence

Le projet doit stocker au minimum :

```text id="jtk26v"
Project

SceneGraph

SceneIntent

asset references

poses

camera

lights

style

history

render settings
```

Les fichiers lourds ne doivent pas nécessairement être dupliqués.

---

# 325. Final project structure

```text id="5u0b9e"
my-illustration/
│
├── project.yaml
├── scene.json
├── intent.json
├── history.log
│
├── overrides/
│   ├── poses/
│   └── materials/
│
├── previews/
│
└── renders/
```

Les assets partagés restent dans l'Asset Registry.

---

# 326. Final capability model

Le module demande uniquement les capabilities nécessaires.

```text id="5h42mo"
scene.read

scene.write

asset.read

render.preview

render.final

fs.read:/illustrations/**

fs.write:/illustrations/**
```

Optionnel :

```text id="7nt9ye"
network.fetch
```

pour rechercher/télécharger de nouveaux assets.

---

# 327. Recommended permission UX

À l'installation :

```text id="jqizsl"
Illustration Studio wants to:

✓ Create and edit 3D scenes
✓ Read installed 3D assets
✓ Save illustration projects
✓ Use GPU rendering
✓ Run the installed Illustration Renderer

○ Access network for additional assets
```

Le réseau peut rester désactivé.

---

# 328. Offline target

Avec :

```text id="1pj6tb"
local LLM

local asset pack

local Blender renderer
```

tout le workflow doit fonctionner :

```text id="f7m3zz"
100% offline
```

C'est parfaitement aligné avec Akasha OS.

---

# 329. Distribution

Je recommanderais trois packages distincts.

```text id="ip33tf"
Akasha OS

Illustration Studio Module

Illustration Renderer Pack
```

et éventuellement :

```text id="2mgqyf"
Illustration Basic Asset Pack
```

---

# 330. Why separate renderer

Blender et les ressources NPR peuvent être volumineux.

L'utilisateur qui n'utilise jamais Illustration Studio n'a aucune raison de les télécharger.

Installation :

```text id="e3alhd"
Install Illustration Studio
        │
        ▼
Renderer missing
        │
        ▼
Install Illustration Renderer Pack?
```

---

# 331. Why separate assets

Même principe :

```text id="4pkf9s"
Basic Assets
```

peut être installé avec le module.

Des packs supplémentaires peuvent ensuite exister :

```text id="h4fup5"
Medieval

Modern City

Fantasy

Sci-Fi

Nature

Victorian

Japanese Interior
```

---

# 332. Community extension points

La communauté doit pouvoir créer :

```text id="z8whp9"
Asset Packs

Style Packs

Pose Packs
```

sans modifier le code du module.

À terme :

```text id="w59w1x"
Environment Packs
```

pourrait également être supporté.

---

# 333. Security rule for packs

Un pack :

```text id="vkhj46"
Asset

Style

Pose
```

est essentiellement déclaratif.

Il ne doit pas pouvoir embarquer arbitrairement :

```text id="u65z85"
executables

scripts

network calls
```

Les extensions nécessitant du code doivent passer par le système de modules/capabilities.

---

# 334. Product positioning

Illustration Studio ne doit pas être présenté comme :

> AI image generator.

Mais plutôt comme :

> **AI-assisted illustration workspace based on editable 3D composition.**

Sa proposition de valeur est :

```text id="4rzz4r"
Generative speed
+
3D control
+
human editing
+
reproducible rendering
```

---

# 335. Key differentiator

Le différenciateur fondamental peut se résumer par :

```text id="45du4l"
Traditional generation

Prompt
 ↓
Image
 ↓
Try again


Illustration Studio

Prompt
 ↓
Scene
 ↓
Edit
 ↓
Render
 ↓
Edit
 ↓
Render
```

L'itération porte sur une **structure**, pas sur le hasard d'une nouvelle génération.

---

# 336. Long-term vision

Illustration Studio peut devenir le premier exemple d'une nouvelle catégorie d'applications Akasha :

```text id="bcw0g4"
Agent-native creative applications
```

où :

```text id="9dmvnb"
human

+

agent

+

structured document

+

specialized runtime
```

travaillent ensemble.

---

# 337. Future ecosystem

La même architecture peut ensuite permettre :

```text id="2ph7d7"
Illustration Studio

Storyboard Studio

Comic Studio

Scene Designer

Game Level Designer

Architecture Studio

Product Visualization

Animation Studio
```

tous basés sur :

```text id="pk0shn"
SceneGraph
+
scene3d
+
Agent Tools
```

---

# 338. Strategic implication for Akasha OS

Le projet ne doit donc pas être vu uniquement comme :

```text id="t7xuhz"
"add an illustration module"
```

mais également comme une validation de :

```text id="b58zfn"
Rich Applications on Akasha OS
```

Il répond à la question :

> Comment un agent peut-il utiliser une application complexe avec l'utilisateur sans donner au module un navigateur ou un environnement de code arbitraire ?

Réponse proposée :

```text id="8tpij5"
Semantic tools
+
Rich declarative widgets
+
Shared structured state
```

---

# 339. The Akasha application model

Le pattern devient :

```text id="04b3qi"
                   Application State
                         │
              ┌──────────┴──────────┐
              │                     │
              ▼                     ▼
            Human                 Agent
              │                     │
              ▼                     ▼
         Rich Widget          Semantic Tools
              │                     │
              └──────────┬──────────┘
                         │
                         ▼
                  Shared State
```

Pour Illustration Studio :

```text id="r67bcr"
Shared State = SceneGraph
```

Pour d'autres applications :

```text id="7whjmx"
Music Studio    → SongGraph

CAD             → DesignGraph

Workflow Editor → WorkflowGraph

Video Editor    → TimelineGraph
```

C'est potentiellement une abstraction importante pour Akasha OS.

---

# 340. Final development roadmap

## Stage A — Foundation

```text id="aof2pr"
Rich Widget architecture

SceneGraph

scene3d

basic viewport
```

**Goal:** afficher et modifier une scène 3D dans Akasha.

---

## Stage B — Characters

```text id="9zyxkg"
Rig

FK

IK

Pose presets

Interaction anchors
```

**Goal:** manipuler naturellement un personnage.

---

## Stage C — Rendering

```text id="i5cqdt"
RenderService

Blender backend

render passes

Sketch

Pencil

Ink
```

**Goal:** transformer la scène en illustration.

---

## Stage D — Illustration Studio

```text id="8e61r1"
Project management

Compose

Pose

Camera

Light

Style

Render
```

**Goal:** workflow manuel complet.

---

## Stage E — AI Composition

```text id="u25av7"
SceneIntent

Scene Planner

Asset Resolver

Pose Planner

Composition Planner
```

**Goal:** prompt → scène.

---

## Stage F — Agent Editing

```text id="us9c55"
semantic tools

transactions

locks

natural language editing
```

**Goal:** humain + agent éditent la même scène.

---

## Stage G — Advanced Illustration

```text id="c63zbp"
Watercolor

Marker

Charcoal

Style packs

better assets

facial expressions
```

---

## Stage H — Creative Platform

```text id="wmhy13"
Canvas integration

Storyboards

Comics

animation

community ecosystem
```

---

# 341. Go / No-Go checkpoints

Afin d'éviter d'investir trop tôt dans le mauvais axe, placer quatre checkpoints.

### Gate 1

Après `scene3d`.

Question :

> Akasha peut-il héberger correctement une application 3D interactive dans son modèle declarative_ui ?

Si non, revoir l'architecture Rich Widgets.

---

### Gate 2

Après rigging.

Question :

> La manipulation d'un personnage est-elle suffisamment intuitive pour quelqu'un qui ne connaît pas Blender ?

Si non, améliorer l'abstraction avant de poursuivre.

---

### Gate 3

Après NPR.

Question :

> Les rendus sont-ils suffisamment proches d'une illustration pour justifier l'approche 3D ?

Si non, revoir le pipeline NPR avant d'ajouter l'IA.

---

### Gate 4

Après Prompt → Scene.

Question :

> Le prompt réduit-il réellement le temps nécessaire pour construire une composition ?

Si oui, le concept complet est validé.

---

# 342. Success metrics

Le succès ne doit pas être mesuré uniquement sur la qualité esthétique.

Mesures importantes :

### Generation

```text id="4brgg0"
Prompt → usable scene time
```

### Editing

```text id="gtq7r4"
time to correct pose
```

### Agent

```text id="mj7l66"
semantic edit success rate
```

### Rendering

```text id="mt5zb5"
preview latency
```

### Control

```text id="fjidwy"
percentage of requested edits
performed without regeneration
```

Cette dernière métrique est particulièrement importante.

---

# 343. Target control metric

Objectif à terme :

> Une grande majorité des demandes de correction portant sur la composition, la pose, la caméra ou la lumière doivent être réalisables **sans régénérer la scène entière**.

C'est précisément ce qui différencie Illustration Studio d'un générateur d'images.

---

# 344. Reference MVP scenario

Le scénario officiel de validation reste :

```text id="qgyytx"
An old bookstore.

A man enters through the door.

A cat lies on the counter.

The cat watches the man.
```

Après génération, le testeur demande successivement :

```text id="fgwxpq"
Move the cat closer to the edge.

Make the man look at the cat.

Put his left hand on the door handle.

Lower the camera.

Use a wider lens.

Make the light warmer.

Render as pencil.

Render as ink.
```

Aucune de ces opérations ne doit reconstruire toute la scène.

---

# 345. Final architecture summary

```text id="b8opjv"
USER PROMPT
     │
     ▼
SCENE PLANNER
     │
     ▼
SCENE INTENT
     │
     ▼
SEMANTIC / CONSTRAINT SOLVERS
     │
     ├── Asset Resolver
     ├── Layout Solver
     ├── Pose / IK Solver
     └── Camera Solver
     │
     ▼
SCENE GRAPH
     │
 ┌───┴────────────────────────────┐
 │                                │
 ▼                                ▼
HUMAN                            AGENT
 │                                │
scene3d                       semantic tools
 │                                │
 └───────────────┬────────────────┘
                 │
                 ▼
             SCENE GRAPH
                 │
                 ▼
           RENDER SERVICE
                 │
                 ▼
          BLENDER BACKEND
                 │
                 ▼
           RENDER PASSES
                 │
                 ▼
             NPR STYLE
                 │
                 ▼
           ILLUSTRATION
```

---

---

# 346. Final decisions

Les choix structurants proposés par cette spec sont donc :

1. **La 3D est la source de vérité.**

2. **Le rendu est dérivé et non destructif.**

3. **Akasha reçoit un nouveau rich widget natif `scene3d`.**

4. **Pas de WebView pour implémenter l'éditeur.**

5. **Le SceneGraph est indépendant du renderer.**

6. **GLB/glTF est utilisé pour les assets, pas comme format de projet.**

7. **Blender headless est le premier backend de rendu, mais reste interchangeable.**

8. **Les styles d'illustration sont déclaratifs et indépendants du SceneGraph.**

9. **Le MVP commence avec Sketch, Pencil et Ink.**

10. **Les personnages utilisent des rigs Akasha normalisés indépendants des assets.**

11. **IK, FK, LookAt et contraintes sont calculés par des solveurs déterministes.**

12. **Le LLM exprime des intentions sémantiques et non des coordonnées 3D arbitraires.**

13. **SceneIntent constitue la représentation intermédiaire entre langage naturel et SceneGraph.**

14. **Les relations spatiales sont explicites : `ON`, `NEAR`, `LOOKING_AT`, `HOLDING`, etc.**

15. **Les assets peuvent exposer des anchors et affordances sémantiques.**

16. **L'humain et l'agent manipulent exactement le même SceneGraph.**

17. **Toutes les modifications humaines ou agentiques utilisent le même système d'opérations.**

18. **Toute opération agentique est auditable, transactionnelle et annulable.**

19. **Les locks permettent à l'utilisateur de protéger ses décisions contre les régénérations de l'agent.**

20. **Les rendus sont liés à une révision précise de la scène et sont reproductibles.**

21. **Les render passes peuvent être mis en cache afin de changer de style sans recalculer inutilement la scène.**

22. **Le renderer fonctionne derrière un `RenderService` capability-scoped.**

23. **Blender n'obtient aucun accès implicite au filesystem, au réseau ou au Vault Akasha.**

24. **Le fonctionnement offline est une capacité de premier ordre.**

25. **La génération IA de meshes 3D n'est pas une dépendance du MVP.**

26. **Une petite bibliothèque d'assets homogènes est préférable à une énorme bibliothèque incohérente.**

27. **La génération d'image neuronale n'est pas nécessaire au fonctionnement du produit.**

28. **Un neural renderer pourra éventuellement être ajouté comme post-process optionnel et structurellement conditionné.**

29. **Les fonctionnalités génériques 3D appartiennent au runtime Akasha ; les fonctionnalités artistiques appartiennent à Illustration Studio.**

30. **`scene3d` doit être conçu comme la première brique d'une architecture générique de Rich Declarative Applications pour Akasha OS.**

---

# 347. Final MVP

Le premier produit réellement utilisable doit permettre ce workflow et rien de fondamentalement plus complexe :

```text id="eivhj3"
PROMPT
   │
   ▼
SCENE
   │
   ▼
COMPOSE
   │
   ▼
POSE
   │
   ▼
CAMERA
   │
   ▼
LIGHT
   │
   ▼
STYLE
   │
   ▼
RENDER
```

L'utilisateur ne doit jamais avoir besoin d'ouvrir Blender.

---

# 348. MVP feature set

```text id="c3cbaw"
Prompt → SceneIntent

SceneIntent → SceneGraph

Semantic Asset Resolver

GLB assets

Proxy geometry

3D viewport

Scene hierarchy

Selection

Move / Rotate / Scale

Humanoid rig

Basic quadruped rig

FK

IK

LookAt

Pose presets

Camera editing

Basic lighting

Sketch NPR

Pencil NPR

Ink NPR

Preview render

Final render

PNG export

Save / Load

Undo / Redo

Agent semantic editing

Offline operation
```

---

# 349. First implementation target

Le développement ne doit cependant **pas commencer par cette liste complète**.

Le premier prototype doit uniquement démontrer :

```text id="g54psu"
Akasha OS
   │
   ▼
declarative_ui
   │
   ▼
scene3d
   │
   ▼
┌─────────────────────────────┐
│                             │
│          MANNEQUIN          │
│                             │
│       rig + IK handles      │
│                             │
│            TABLE            │
│                             │
└─────────────────────────────┘
```

avec :

```text id="hjifup"
orbit camera

select object

move object

select skeleton

move hand IK target

save scene
```

Si cette expérience est fluide et compatible avec le modèle de sécurité Akasha, le projet possède ses fondations.

---

# 350. Second implementation target

Ajouter ensuite :

```text id="uywd03"
SceneGraph
       │
       ▼
RenderService
       │
       ▼
Blender
       │
       ▼
┌───────────────┐
│   3D Render   │
├───────────────┤
│    Sketch     │
├───────────────┤
│    Pencil     │
├───────────────┤
│      Ink      │
└───────────────┘
```

Cette étape valide le deuxième pari technique : **une scène 3D contrôlée peut produire des illustrations suffisamment convaincantes.**

---

# 351. Third implementation target

Seulement ensuite ajouter :

```text id="n3ccpa"
Prompt
 ↓
SceneIntent
 ↓
Scene Composer
 ↓
SceneGraph
```

Le LLM arrive donc **après** la validation du runtime 3D et du renderer.

---

# 352. First full demonstration

Le premier véritable prototype produit doit être capable de réaliser :

```text id="smm80a"
"A man enters an old bookstore
while a cat lies on the counter."
```

et de permettre ensuite :

```text id="dnpyim"
"Make the man look at the cat."

"Put his hand on the door."

"Move the cat closer to the edge."

"Lower the camera."

"Make the lighting warmer."

"Render it as pencil."
```

Le tout sans régénérer entièrement la scène.

---

# 353. Definition of success

Le projet est techniquement réussi lorsque :

> une personne qui ne maîtrise pas un logiciel 3D peut transformer une idée décrite en langage naturel en une composition 3D, la corriger visuellement, manipuler les poses de ses personnages et produire une illustration stylisée sans avoir à comprendre les concepts techniques d'un logiciel comme Blender.

Le projet est architecturalement réussi lorsque :

> Illustration Studio n'est qu'un consommateur de primitives Akasha génériques suffisamment propres pour permettre ensuite la création d'autres applications riches.

---

# 354. Final vision

L'objectif final n'est donc pas simplement :

```text id="v69yjm"
AI → Illustration
```

mais :

```text id="6qll60"
                 HUMAN
                   │
                   ▼
            ┌─────────────┐
            │ STRUCTURED  │
            │   WORLD     │
            └──────┬──────┘
                   ▲
                   │
                 AGENT

                   │
                   ▼

             ILLUSTRATION
```

L'IA ne remplace pas l'outil créatif.

Elle aide à **construire et manipuler le monde structuré sur lequel l'utilisateur garde le contrôle**.

C'est précisément ce qui distingue cette approche d'un générateur d'images traditionnel.

---

# 355. Specification conclusion

`Illustration Studio` doit être développé comme le premier cas d'usage majeur d'une nouvelle capacité d'Akasha OS :

> **Rich, agent-native, structured applications.**

Son architecture repose sur trois idées :

```text id="7v2g1u"
Rich Declarative UI
        +
Shared Structured State
        +
Semantic Agent Tools
```

Pour ce module :

```text id="1mzt1l"
scene3d
        +
SceneGraph
        +
scene / rig / camera / render tools
```

Cette architecture permet à l'utilisateur et à l'agent de travailler ensemble sur **le même artefact**, avec contrôle, auditabilité, sécurité et réversibilité.

Le développement doit commencer par le runtime `scene3d`, pas par l'IA.

Le chemin recommandé est :

```text id="5nqfbx"
scene3d
   ↓
SceneGraph
   ↓
Rig / IK
   ↓
RenderService
   ↓
NPR
   ↓
Illustration Studio
   ↓
SceneIntent
   ↓
AI composition
   ↓
Agent editing
```

Une fois cette chaîne validée, Akasha OS disposera non seulement d'un module d'illustration, mais d'une fondation réutilisable pour toute une classe d'applications créatives et techniques agent-native.

**End of specification — v0.1**

Oui. Et après vérification, je modifierais la spec en ajoutant **trois gates P0 avant même `scene3d`**.

### 356. P0 — Numeric conventions

À figer immédiatement dans un ADR, parce que tout `SceneGraph`, rig, asset et backend en dépend :

```yaml
coordinate_system:
  handedness: right
  up_axis: Y
  forward_axis: -Z

units:
  distance: meter
  angle_serialization: radian
  angle_ui: degree

transform:
  position: vec3<f32>
  rotation: quaternion<f32>
  scale: vec3<f32>

matrix:
  convention: column-major

quaternion:
  order: [x, y, z, w]

camera:
  focal_length: millimeter
  sensor_width: millimeter

color:
  working_space: linear
  ui_input: sRGB

time:
  unit: second
```

J'ajouterais surtout une règle :

> **Les conventions du SceneGraph sont celles d'Akasha, jamais celles du backend.**

Donc le Blender Adapter porte explicitement la conversion :

```text
Akasha coordinates
        ↓
BackendTransform
        ↓
Blender coordinates
```

On ne laisse jamais les conventions Blender « fuiter » dans `aos-scene`.

Je mettrais également dans les tests golden :

```text
identity transform
90° rotation X/Y/Z
parent + child transform
camera forward vector
rig rest pose
IK target
GLTF → Akasha → Blender round-trip
```

**Gate P0-A :** aucune crate `aos-scene` avant validation de cet ADR.

---

# 357. P0 — Eevee headless sandbox spike

Là, tu as identifié le vrai risque caché de ma première spec.

La documentation Blender confirme que le rendu `--background` fonctionne sans interface graphique et, sous Linux, sans serveur X. :chatgpt-content-reference{index="0"}

Mais ça ne prouve **pas** notre scénario réel :

```text
Akasha sandbox
      ↓
Blender background
      ↓
Eevee
      ↓
GPU
      ↓
NVIDIA / Metal / autres
```

Il faut donc faire le spike **avant RenderService**.

Je créerais un ticket technique :

**`SPIKE — Validate Blender Eevee headless rendering inside Akasha sandbox`**

Matrice minimale :

| Platform | Mode | Expected |
|---|---|---|
| Windows 11 | NVIDIA GPU | Eevee headless |
| Windows 11 | CPU/fallback | graceful fallback |
| Linux | NVIDIA GPU, no X | Eevee headless |
| macOS Apple Silicon | Metal | Eevee headless |
| Sandbox | network denied | render works |
| Sandbox | restricted FS | render works |

Le test doit lancer une scène contrôlée :

```text
Cube
Plane
Camera
Area Light
Texture
Line Art / NPR test
```

et produire :

```text
beauty.png
depth.exr
normal.exr
object_id.exr
```

Puis vérifier :

```text
exit code
render output
GPU actually used
startup latency
peak VRAM
peak RAM
render duration
filesystem accesses
network accesses
stderr
```

Le point GPU mérite vraiment le test empirique : la documentation décrit les mécanismes GPU, mais ce que nous devons valider est leur fonctionnement dans **notre isolation et sur nos trois plateformes**, pas seulement dans Blender normalement installé. :chatgpt-content-reference{index="1"}

### Gate P0-B

On continue avec Blender/Eevee uniquement si :

```text
headless       PASS
sandbox        PASS
Win NVIDIA     PASS
Linux NVIDIA   PASS
macOS Metal    PASS
NPR passes     PASS
```

Sinon, la spec ne doit pas s'effondrer : `RenderService` reste valide et on remplace le backend.

C'est justement une bonne justification supplémentaire pour **ne jamais coupler `Illustration Studio` à Blender**.

---

# 358. P0 — Blender GPL / Renderer Pack

Et celui-ci doit effectivement être tranché **avant de décider que le Renderer Pack contient Blender**.

Blender est GPL ; la Blender Foundation indique que la distribution des binaires Blender entraîne les obligations GPL correspondantes. Pour une release officielle redistribuée, elle indique notamment qu'on peut renvoyer vers les sources Blender ; une version modifiée impose évidemment la disponibilité des modifications correspondantes. :chatgpt-content-reference{index="2"}

Il y a surtout une frontière architecturale très intéressante pour Akasha. La FAQ Blender indique qu'un logiciel externe peut conserver sa propre licence lorsqu'il fonctionne **hors de Blender**, n'utilise ni son code source ni son API, produit des données destinées à Blender, puis exécute Blender pour que celui-ci lise ces données. :chatgpt-content-reference{index="3"}

Ça correspond presque exactement à l'architecture souhaitée :

```text
Akasha / Illustration Studio
          │
          │ Scene/Render package
          ▼
     process boundary
          │
          ▼
        Blender
          │
          ▼
        images
```

En revanche, il y a un piège important : la Blender Foundation considère les scripts Python publiés utilisant `bpy` comme devant être distribués sous une licence compatible GPL. :chatgpt-content-reference{index="4"}

Donc je **ne mettrais pas** tout ceci dans un même composant Apache :

```text
aos-render-blender
 └── adapter.py using bpy
```

en supposant que l'Apache-2.0 actuel des modules Akasha règle le problème.

Je séparerais plutôt :

```text
AKASHA OS
AGPL-3.0
│
├── RenderService
│
└── Blender protocol/client
           │
           │ process + files
           ▼
────────────────────────────── LICENSE BOUNDARY
           │
Blender Renderer Pack
│
├── Blender binary        GPL
├── bpy adapter           GPL-compatible
├── NPR Blender scripts   GPL-compatible
├── LICENSES/
└── SOURCE-OFFER / source information
```

Les **styles purement data**, textures dont nous détenons les droits, SceneGraph, protocoles et sorties n'ont pas besoin d'être artificiellement assimilés au code Blender ; il faut néanmoins traiter précisément chaque composant selon sa licence.

Et les illustrations produites par Blender ne deviennent pas GPL pour autant : Blender précise que les créations produites avec le logiciel restent la propriété de leur auteur/utilisateur. :chatgpt-content-reference{index="5"}

Je ferais cependant valider le packaging final par un juriste OSS avant distribution publique : Blender lui-même recommande de consulter un juriste pour un avis juridique GPL. :chatgpt-content-reference{index="6"}

### Gate P0-C

Avant de publier un `Illustration Renderer Pack` :

```text
[ ] dependency inventory
[ ] license of every component
[ ] bpy scripts licensing decided
[ ] Blender source availability mechanism
[ ] GPL notices included
[ ] third-party notices included
[ ] Akasha ↔ Blender process boundary documented
[ ] no Blender API linkage from non-GPL component
[ ] redistribution model reviewed
```

---

## Donc le début de roadmap change

Je remplacerais le début actuel par :

```text
P0-A  Numeric conventions ADR
 │
 ├───────────────┐
 ▼               ▼
P0-B            P0-C
Eevee           Blender
sandbox spike   GPL review
 │               │
 └───────┬───────┘
         ▼
      GO / NO-GO
         │
         ▼
ADR Rich Declarative Widgets
         │
         ▼
scene3d spike
         │
         ▼
SceneGraph
         │
        ...
```

Et il y a une subtilité : **P0-A peut être fermé immédiatement**, tandis que **P0-B et P0-C peuvent avancer en parallèle**.

