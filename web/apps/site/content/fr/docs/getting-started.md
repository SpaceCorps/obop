---
title: "Premiers pas"
description: "Lancez votre première instance obop : auto-hébergez avec Docker et PostgreSQL, ou exécutez-la en local avec SQLite. Installez ensuite une application, créez des enregistrements et invitez un agent."
lead: "Deux façons d'exécuter obop : Docker avec PostgreSQL pour un vrai déploiement, ou une compilation locale pour le développement. obop.dev ne propose pas encore d'instances hébergées."
---

SpaceCorps n'héberge pas encore obop pour d'autres personnes : c'est donc vous qui faites tourner obop. Un bac à sable gratuit avec des données d'exemple et un hébergement payant sur serveurs dédiés sont prévus ; aucun des deux n'est encore ouvert, et aucune date n'est promise. Voir [Tarifs](/pricing/).

## Option 1 : auto-hébergement avec Docker et PostgreSQL

Vous avez besoin de Docker. Le dépôt contient un fichier Compose qui démarre PostgreSQL et obop ensemble, exposés uniquement sur localhost :

```bash
git clone https://github.com/SpaceCorps/obop.git
cd obop
docker compose up --build
```

La première compilation construit tout l'espace de travail Rust et prend un certain temps. Une fois obop démarré, ouvrez `http://127.0.0.1:8080/admin/`. Le fichier Compose lance obop avec des données de démonstration (`--demo`) et sans aucun utilisateur ; créez donc le premier administrateur :

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

Par défaut, l'image exécute une seule instance. Sa commande est `obop serve --addr 0.0.0.0:8080 --static /srv/backoffice --public-site --trust-proxy` : le site public est à `/` et le back-office à `/admin/`. `--trust-proxy` lit l'adresse du client dans `X-Forwarded-For` ; exécutez donc l'image derrière un proxy que vous contrôlez. Créez le premier administrateur avec `docker exec -i obop obop admin create alice --all-groups`. Consultez [Auto-hébergement et exploitation](/docs/reference/self-hosting/) pour le plan de contrôle, les sauvegardes et ce qui manque.

## Option 2 : développement local avec SQLite

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

## Après la connexion

1. **Installez une application.** Ouvrez _Personnaliser, Modules_ puis l'onglet _Place de marché_, et installez une application telle qu'Assistance ou Projets. L'installation et la suppression d'applications nécessitent le groupe administrateur. Les applications intégrées sont déjà là, parmi elles Contacts, Produits, CRM, Ventes, Facturation et Connecteurs (les six qui ont une page d'application).
2. **Créez des enregistrements.** Ouvrez une application, choisissez _Nouveau_ et remplissez le formulaire. Les listes, les tableaux et le panneau latéral sont générés à partir de la description de l'application.
3. **Invitez un agent.** Il n'existe pas encore d'écran pour les jetons d'agent. Connectez-vous via l'API REST et [créez un jeton d'agent](/docs/agents/tokens/), puis [connectez-vous via MCP](/docs/agents/mcp/) ou utilisez [REST](/docs/agents/rest/).

## Et ensuite

- Découvrez les briques de base dans [Concepts](/docs/concepts/).
- Parcourez les [applications](/apps/) et installez celles dont vous avez besoin depuis _Personnaliser_, ou en ligne de commande avec `obop modules install-from helpdesk`.
- Configurez un agent : [Présentation des agents](/docs/agents/).
- Écrivez votre propre application : [référence du manifeste de module](/docs/reference/module-manifest/).
