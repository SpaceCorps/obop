---
title: "Premiers pas"
description: "Obtenez votre première instance obop : inscrivez-vous à la bêta hébergée, auto-hébergez avec Docker et PostgreSQL, ou exécutez-la en local avec SQLite. Installez ensuite une application, créez des enregistrements et invitez un agent."
lead: "Trois façons d'exécuter obop. Choisissez la bêta hébergée pour essayer en quelques minutes, Docker avec PostgreSQL pour un vrai déploiement, ou une compilation locale pour le développement."
---

## Option 1 : la bêta hébergée

Le service hébergé donne à chaque entreprise sa propre instance, avec sa propre base de données. Il est gratuit pendant la bêta.

1. **Inscrivez-vous.** Ouvrez [l'application d'inscription](/app/), saisissez votre adresse e-mail et un mot de passe, puis choisissez _Create account_.
2. **Créez une instance.** Donnez-lui un nom de 3 à 30 caractères. L'adresse est construite à partir du nom. La mise en place prend un moment ; la page affiche la progression.
3. **Connectez-vous.** Suivez le lien vers votre instance et connectez-vous avec la même adresse e-mail et le même mot de passe. Votre compte d'inscription devient le premier administrateur de l'instance.
4. **Installez une application.** Ouvrez _Customize_, puis _Marketplace_, et installez une application telle que Helpdesk ou Projects. L'installation et la suppression d'applications nécessitent le groupe administrateur. Les cinq applications intégrées (CRM, Sales, Invoicing, Products et Contacts) sont déjà là.
5. **Créez des enregistrements.** Ouvrez une application, choisissez _New_ et remplissez le formulaire. Les listes, les tableaux et le panneau latéral sont générés à partir de la description de l'application.
6. **Invitez un agent.** Il n'existe pas encore d'écran pour les jetons d'agent. Connectez-vous via l'API REST et [créez un jeton d'agent](/docs/agents/tokens/), puis [connectez-vous via MCP](/docs/agents/mcp/) ou utilisez [REST](/docs/agents/rest/).

## Option 2 : auto-hébergement avec Docker et PostgreSQL

Vous avez besoin de Docker. Le dépôt contient un fichier Compose qui démarre PostgreSQL et obop ensemble, exposés uniquement sur localhost :

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

La première compilation construit tout l'espace de travail Rust et prend un certain temps. Une fois obop démarré, ouvrez `http://127.0.0.1:8080`. Le fichier Compose lance obop avec des données de démonstration (`--demo`) et sans aucun utilisateur ; créez donc le premier administrateur :

```bash
docker compose exec obop obop admin create alice --all-groups
```

obop lit le mot de passe sur l'entrée standard : il vous est donc demandé (12 caractères au minimum). Connectez-vous en tant que `alice`.

Pour exécuter l'image avec votre propre instance PostgreSQL, définissez `OBOP_DATABASE_URL` :

```bash
docker build -t obop .
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://user:password@host:5432/obop \
  obop
```

Par défaut, l'image exécute une seule instance. Sa commande est `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice`. Consultez [Auto-hébergement et exploitation](/docs/reference/self-hosting/) pour le plan de contrôle, les sauvegardes et ce qui manque.

## Option 3 : développement local avec SQLite

Vous avez besoin de la chaîne d'outils Rust définie dans `rust-toolchain.toml`, de pnpm et de Vite+ (`vp`). SQLite est le stockage par défaut : aucun serveur de base de données n'est nécessaire.

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
cargo build
vp install
(cd web/apps/backoffice && vp build)
echo "a password of 12+ characters" | cargo run -- admin create alice --all-groups
cargo run -- serve --static web/apps/backoffice/dist
```

Ouvrez `http://127.0.0.1:8080` et connectez-vous en tant que `alice`. Les données se trouvent dans `obop.db`, dans le répertoire courant ; passez `--db sqlite:///path/to/file.db` pour utiliser un autre fichier. Une base SQLite neuve et vide est remplie automatiquement de fausses données de démonstration (`--demo` force ce remplissage).

`--dev-auth` remplace la vraie connexion par des jetons fixes nommés `dev-admin`, `dev-sales` et `dev-agent`. Il refuse de s'attacher à toute adresse autre que localhost.

## Et ensuite

- Découvrez les briques de base dans [Concepts](/docs/concepts/).
- Parcourez les [applications](/apps/) et installez celles dont vous avez besoin depuis _Customize_, ou en ligne de commande avec `obop modules install-from helpdesk`.
- Configurez un agent : [Présentation des agents](/docs/agents/).
- Écrivez votre propre application : [référence du manifeste de module](/docs/reference/module-manifest/).
