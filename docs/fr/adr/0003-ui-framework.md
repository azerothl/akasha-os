# ADR 0003: UI Framework Decision

**Langue :** [English](../../adr/0003-ui-framework.md) | Français


## Contexte

La Phase P3 a validé plusieurs interfaces utilisateur (TUI, web, etc.). Ce ADR formalise le choix du framework UI principal pour l'Agent OS.

## Options considérées

| Framework | Avantages | Inconvénients |
|-----------|-----------|---------------|
| **egui** | Léger, rapide, bon pour les interfaces 2D simples, intégration facile avec Rust | Pas de support natif pour les animations complexes, limitation de la personnalisation avancée |
| **iced** | Performant, moderne, support natif de l'animations, communauté active | Plus complexe à intégrer avec Rust, moins de templates prêts-à-utiliser |
| **tauri** | Application web (HTML/CSS/JS) compilée en Rust, portabilité multi-plateforme | Overhead de compilation, dépendance à un runtime web |

## Décision

**Choix : egui**

- **Raisons** : 
  - Léger et rapide à intégrer dans le cycle de développement
  - Support natif de Rust (bindings stables)
  - Interface 2D suffisante pour l'assistant conversationnel et les dashboards
  - Communauté active et documentation abondante
  - Facilité de prototypage rapide pour les changements d'UX

- **Alternative** : Si les besoins évoluent vers des interfaces web complexes, tauri pourra être ajouté en P5.

## Impact

- **Phase P1** : Intégration de egui dans le Model Subsystem v1
- **Phase P2** : Extension avec des widgets custom (graphiques, tables)
- **Phase P3** : Dashboard complet avec panels de ressources
- **Phase P4** : UI portable sur les machines cibles (ARM64, x86_64)

## Conséquences futures

- **Maintenance** : egui sera maintenu comme base, avec des extensions custom pour les besoins spécifiques
- **Portabilité** : Le code UI sera encapsulé dans un module séparé pour faciliter le déploiement multiplateforme
- **Évolutions** : Si besoin de web-native, tauri pourra être porté en P5

## Références

- [P1.6 UI minimale](../plan-developpement-phases.md)
- [Spécifications fonctionnelles - Interfaces](../specs-fonctionnelles.md)
