# Politique de sécurité

**Langue :** [English](../../SECURITY.md) | Français

N’ouvre **pas** d’issue GitHub publique pour un rapport de sécurité.

## Comment signaler

Écrire à **loic.peaudecerf@proton.me** (objet : `Akasha OS — security`).

Inclure : version Preview (`VERSION` dans le préfixe d’install), OS hôte,
ce que tu as fait, ce qui s’est passé, et si une capacité, un secret ou une
sortie réseau est en jeu. N’attache pas de clés vault live.

## Feedback in-app

L’onglet Feedback accepte la catégorie **security**. Ces rapports **ne
sont pas** publiés en issues GitHub. Une copie locale est écrite sous
`var/feedback/`. Préfère l’e-mail si la trouvaille est sensible.

## Périmètre

Dans le périmètre : contournement de caps, fuite de secrets, install de
module non signé, egress alors que le réseau est off, altération de
l’audit, privilège de `aos-bridged` au-delà du loopback.

Hors périmètre : seL4 / fer nu (absent du zip Preview public), modèles et
serveurs MCP tiers que tu as ajoutés, problèmes qui exigent un attaquant
local déjà maître du compte utilisateur sauf s’ils font monter une cap.

## Attribution

Un correctif te créditera dans les notes de version si tu le souhaites. Pas
de bug bounty pour l’instant.
