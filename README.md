<p align="right"><b>🇫🇷 Français</b> · <a href="README-EN.md">🇬🇧 English</a></p>

<div align="center">

# mcp-pennylane

**Pilotez votre compta Pennylane depuis Claude — factures, journal, banque, FEC**

<br/>

<img src="assets/banner.svg" alt="Claude → mcp-pennylane → Pennylane Company API v2" width="700"/>

<br/>
<br/>

[![crates.io](https://img.shields.io/crates/v/mcp-pennylane.svg)](https://crates.io/crates/mcp-pennylane)
[![downloads](https://img.shields.io/crates/d/mcp-pennylane.svg)](https://crates.io/crates/mcp-pennylane)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/FGRibreau/mcp-pennylane/actions/workflows/ci.yml/badge.svg)](https://github.com/FGRibreau/mcp-pennylane/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://rust-lang.org)
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-6366f1.svg)](https://modelcontextprotocol.io)

</div>

---

## Sponsors

<table>
  <tr>
    <td align="center" width="175">
      <a href="https://france-nuage.fr/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=france-nuage&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/france-nuage.svg" height="60" alt="France-Nuage"/><br/>
        <b>France-Nuage</b>
      </a><br/>
      <sub>Cloud souverain français pour héberger vos exports comptables et archives FEC.</sub>
    </td>
    <td align="center" width="175">
      <a href="https://hook0.com/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=hook0&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/hook0.png" height="60" alt="Hook0"/><br/>
        <b>Hook0</b>
      </a><br/>
      <sub>Diffusez les webhooks Pennylane comme événements signés vers votre back-office.</sub>
    </td>
    <td align="center" width="175">
      <a href="https://getnatalia.com/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=natalia&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/natalia.svg" height="60" alt="Natalia"/><br/>
        <b>Natalia</b>
      </a><br/>
      <sub>Agent vocal IA qui répond aux appels fournisseurs et clients sur les factures.</sub>
    </td>
    <td align="center" width="175">
      <a href="https://www.netir.fr/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=netir&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/netir.svg" height="60" alt="Netir"/><br/>
        <b>Netir</b>
      </a><br/>
      <sub>Recrutez des comptables et finance ops freelances français vérifiés.</sub>
    </td>
  </tr>
  <tr>
    <td align="center" width="233">
      <a href="https://nobullshitconseil.com/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=nbc&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/nobullshitconseil.svg" height="60" alt="NoBullshitConseil"/><br/>
        <b>NoBullshitConseil</b>
      </a><br/>
      <sub>Conseil finance ops et ERP sans bullshit. Spécialiste des intégrations Pennylane.</sub>
    </td>
    <td align="center" width="233">
      <a href="https://qualneo.fr/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=qualneo&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/qualneo.svg" height="60" alt="Qualneo"/><br/>
        <b>Qualneo</b>
      </a><br/>
      <sub>LMS Qualiopi pour formateurs français, avec exports de facturation prêts pour Pennylane.</sub>
    </td>
    <td align="center" width="233">
      <a href="https://www.recapro.ai/?mtm_source=github&mtm_medium=sponsor&mtm_campaign=recapro&mtm_content=mcp-pennylane">
        <img src="assets/sponsors/recapro.png" height="60" alt="Recapro"/><br/>
        <b>Recapro</b>
      </a><br/>
      <sub>IA privée pour transcrire vos rendez-vous comptables et rédiger les comptes-rendus on-prem.</sub>
    </td>
  </tr>
</table>

> **Envie de devenir sponsor ?** [Contactez-nous](mailto:rust@fgribreau.com)

---

## C'est quoi ?

`mcp-pennylane` connecte Claude (ou n'importe quel hôte MCP) à l'[API Pennylane Company v2](https://pennylane.readme.io). Environ 74 opérations essentielles sont exposées directement à l'hôte, et deux méta-outils (`pennylane_search_tools`, `pennylane_execute`) couvrent la longue traîne — la totalité des 163 opérations reste donc utilisable sans saturer le budget d'outils du client MCP.

Pilotez votre compta depuis un chat : lister et créer des factures clients, rapprocher des transactions bancaires, interroger le grand livre, générer les exports FEC pour votre expert-comptable, gérer les mandats GoCardless et SEPA — le tout en langage naturel.

## Fonctionnalités

- ✨ **Toutes les 163 opérations Pennylane** — 74 essentielles directement + `pennylane_search_tools` + `pennylane_execute` pour la longue traîne
- 🔒 **Auto-détection du mode lecture seule** — interroge `GET /me` au démarrage, force le readonly si chaque scope du token finit par `:readonly`
- ⚡ **Deux transports** — stdio pour Claude Desktop, HTTP streamable pour le distant (`/mcp` + `/health`)
- ⚙️ **Codegen via OpenAPI** — `build.rs` parse la spec vendorisée, fail-fast si la whitelist dérive
- 🤖 **Auto-PR hebdomadaire** — une GitHub Action diffe l'upstream tous les lundis et ouvre une PR si la spec a bougé
- 🛡️ **Token redacté** — `Bearer abcd***wxyz` sur tous les chemins de log, les bodies comptables jamais loggés en INFO
- 💡 **Erreurs structurées** — `UNAUTHORIZED`, `VALIDATION_FAILED`, `RATE_LIMITED`, … avec body upstream tronqué et hint actionnable
- 📦 **Binaire statique unique** — `cargo install`, GitHub Releases (linux/macOS/windows × x86_64/aarch64), tap Homebrew

## Démarrage rapide

```bash
# 1. Installer
cargo install mcp-pennylane

# 2. Générer un token Pennylane dans Paramètres → Connectivité → Développeurs
#    Scope recommandé : « Lecture seule — récupérer les données »
export PENNYLANE_API_KEY="votre-token-pennylane"

# 3. Brancher dans Claude Desktop, puis redémarrer Claude
cat <<'EOF' >> ~/Library/Application\ Support/Claude/claude_desktop_config.json
{
  "mcpServers": {
    "pennylane": {
      "command": "/usr/local/bin/mcp-pennylane",
      "env": { "PENNYLANE_API_KEY": "votre-token-pennylane" }
    }
  }
}
EOF
```

C'est tout — Claude voit maintenant `getMe`, `getCustomers`, `pennylane_search_tools`, et 70+ autres outils.

> 💡 Le serveur interroge `GET /me` au démarrage. Si votre token est en lecture seule, il active automatiquement le mode readonly (la bannière affiche `mode=readonly (auto)`). Définissez `PENNYLANE_READONLY=true` pour ignorer la sonde et forcer le readonly de manière déterministe (recommandé en CI).

## Configuration

### Variables d'environnement

| Variable | Défaut | Rôle |
|---|---|---|
| `PENNYLANE_API_KEY` | *requis* | Token Bearer (Paramètres → Connectivité → Développeurs) |
| `PENNYLANE_BASE_URL` | `https://app.pennylane.com` | Override pour proxy ou éventuelle URL régionale |
| `PENNYLANE_READONLY` | *(auto-détecté)* | `true` / `false` pour court-circuiter la sonde `/me` |
| `PENNYLANE_ENV` | `production` | `production` / `sandbox` — repère visuel dans la bannière et dans `getMe` |
| `PENNYLANE_API_2026` | `false` | Envoie l'en-tête `X-Use-2026-API-Changes: true` (phase preview) |
| `PENNYLANE_UPLOAD_DIRS` | *(aucune restriction)* | Dossiers d'où un fichier peut être envoyé, séparés par `:` (`;` sous Windows). Voir [Envoi de fichiers](#envoi-de-fichiers) |
| `MCP_PENNYLANE_TRANSPORT` | `stdio` | `stdio` ou `http` |
| `RUST_LOG` | `info` | Filtre standard `tracing-subscriber` |

### Flags CLI

Chaque variable d'environnement a un `--flag` équivalent (`--token`, `--base-url`, `--readonly`, `--env`, `--api-2026`, `--transport`, `--host`, `--port`, `--log-level`, `--upload-dirs`). Lancez `mcp-pennylane --help` pour la liste complète.

## Installation

| Canal | Commande |
|---|---|
| crates.io | `cargo install mcp-pennylane` |
| Homebrew | `brew install fgribreau/tap/mcp-pennylane` |
| GitHub Releases | [Télécharger l'archive](https://github.com/FGRibreau/mcp-pennylane/releases/latest) (linux/macOS/windows × x86_64/aarch64) |

## Utilisation

Trois workflows de bout en bout que vous pouvez piloter depuis Claude :

**1. Lister les factures impayées récentes.** *« Montre-moi mes factures clients impayées des 30 derniers jours. »* → Claude appelle `getCustomerInvoices` avec un filtre du type `[{"field":"status","operator":"eq","value":"unpaid"},{"field":"date","operator":"gteq","value":"2026-04-06"}]`, parcourt la pagination via `next_cursor` si nécessaire.

**2. Rapprocher une transaction bancaire.** *« Trouve une transaction de 1 250 € non rapprochée cette semaine et lie-la à la bonne facture. »* → Claude appelle `getTransactions`, puis `pennylane_search_tools(query="match")` pour découvrir l'opération de rapprochement, puis `pennylane_execute(tool_name="postCustomerInvoiceMatchedTransactions", params={"id": <id_facture>, "transaction_id": <id_tx>})`.

**3. Générer un FEC pour le T1 2026.** *« Génère le FEC de janvier à mars 2026 et préviens-moi quand il est prêt. »* → Claude appelle `exportFec` avec `{"start_date":"2026-01-01","end_date":"2026-03-31"}`, polle `getFecExport` jusqu'à ce que le statut passe à `done`, puis renvoie l'URL de téléchargement.

## Mode lecture seule

`mcp-pennylane` résout la posture readonly en deux temps :

1. **Auto-détection** (par défaut) — quand `PENNYLANE_READONLY` n'est pas défini, le serveur interroge `GET /me` une fois au démarrage, lit le tableau `scopes` du token, et force le readonly **uniquement si** chaque scope finit par `:readonly`. La bannière affiche `mode=readonly (auto)` ou `mode=read+write (auto)`.
2. **Override explicite** — `PENNYLANE_READONLY=true` (ou `false`) court-circuite complètement la sonde. Bannière : `mode=… (explicit)`. Utile en CI pour un démarrage déterministe.

Quelle que soit l'origine, lorsque le readonly est actif, les opérations d'écriture sont filtrées au moment de l'enregistrement ET `pennylane_execute` renvoie une erreur structurée `READONLY_MODE` avant tout appel HTTP vers Pennylane.

```bash
# Defense-in-depth : explicite + scope du token Pennylane en lecture seule
export PENNYLANE_API_KEY=…                # token créé avec « Lecture seule »
export PENNYLANE_READONLY=true            # filtre serveur explicite
mcp-pennylane
```

Le scope du token Pennylane est le radio bouton à la création du token dans **Paramètres → Connectivité → Développeurs**. Une couche protège contre un serveur mal configuré, l'autre contre un token mal configuré.

## Envoi de fichiers

Huit opérations de l'API Pennylane exigent `multipart/form-data` : `postFileAttachments` (dépose une pièce et renvoie son `id` et son `url`, le `file_attachment_id` à passer à `importSupplierInvoice`), `createSupplierInvoiceEInvoiceImport` (outil direct), puis, via `pennylane_execute`, `postLedgerAttachments`, `createCustomerInvoiceEInvoiceImport`, `createEInvoiceImport`, `postCustomerInvoiceAppendices`, `postQuoteAppendices` et `postCommercialDocumentAppendices`.

Le champ `file` est le **chemin absolu d'un fichier de la machine qui exécute le serveur** : c'est le serveur qui lit le fichier, pas le client. Avec le transport HTTP, ce n'est donc pas la machine du client MCP.

```json
{
  "tool_name": "postFileAttachments",
  "params": { "file": "/Users/moi/Factures/octobre.pdf", "filename": "Facture octobre 2026.pdf" }
}
```

La réponse est le JSON de Pennylane tel quel, plus une clé `_mcp_pennylane.uploaded_file` (chemin réel lu, nom, taille, type) qui montre à la personne derrière l'agent quel fichier local est parti.

### Contrôles avant l'envoi

Ils s'exécutent en local, sans réseau, dans cet ordre. Un fichier qui en échoue un n'est jamais envoyé.

- le chemin est absolu : `~` et les chemins relatifs sont refusés ;
- le chemin est sous un dossier de `PENNYLANE_UPLOAD_DIRS` quand cette variable existe ;
- les liens symboliques sont résolus avant tout contrôle : le dossier, la taille et le type sont jugés sur le fichier réel ;
- le fichier est ordinaire (ni dossier ni périphérique) ;
- il pèse au plus 100 Mo, mesuré sur le fichier ouvert, la lecture s'arrêtant au plafond : un fichier qui grossit pendant la lecture est refusé ;
- son type (PDF, PNG, JPEG, TIFF, BMP, GIF) est lu sur ses octets, jamais sur l'extension : une clé SSH ou un `.env` renommé en `.pdf` est refusé ;
- `filename` fait au plus 255 caractères, sans caractère de contrôle, y compris quand il vient du nom du fichier local.

Les champs inconnus, les champs requis et leurs valeurs permises sont aussi vérifiés avant que le fichier soit lu.

La lecture du fichier local est limitée à 30 s (fichier encore en téléchargement depuis un cloud, montage réseau qui ne répond pas) et le serveur traite au plus 4 envois à la fois, soit 400 Mo de mémoire au pire : un cinquième attend un créneau 30 s au plus, puis reçoit l'erreur `UPLOADS_BUSY`.

### Restreindre les dossiers : `PENNYLANE_UPLOAD_DIRS`

```bash
export PENNYLANE_UPLOAD_DIRS="$HOME/Factures:$HOME/Downloads"
```

Liste de dossiers absolus séparés par `:` (`;` sous Windows), au plus 32, résolus une fois au démarrage. Un dossier absent, relatif ou qui n'est pas un dossier, ou une variable définie mais vide, empêche le démarrage. Variable absente : aucun dossier imposé.

**Posez-la dès qu'un agent lit des mails ou d'autres contenus non fiables.** Une consigne cachée dans un message peut lui faire désigner n'importe quel fichier lisible (contrat, bulletin de paie) ; le contrôle des octets ne l'arrête pas si c'est un PDF. Le fichier atterrirait dans le dossier Pennylane que le cabinet comptable consulte.

### Un envoi n'est pas idempotent

Renvoyer le même fichier peut créer une seconde pièce. Réutilisez le `file_attachment_id` reçu au lieu de renvoyer. Après une erreur réseau, `NETWORK_ERROR` précise que le fichier a peut-être été reçu : cherchez la pièce avant de renvoyer. Le délai d'un envoi est de 60 s plus 1 s par Mo, pour éviter qu'un envoi réussi côté Pennylane mais lent soit pris pour un échec. Un 429 est rejoué une fois, sans risque de doublon puisque Pennylane a refusé la requête.

Une erreur 403 sur ces opérations cite les scopes requis (`file_attachments:all` pour `postFileAttachments`) ; `getMe` montre ceux du token. Le mode lecture seule les refuse toutes.

### Imports Factur-X

`invoice_options` est envoyé en partie JSON (`application/json`), comme la spec OpenAPI le décrit. **Ce format n'a pas été vérifié en réel contre Pennylane.** Le serveur contrôle le type de fichier mais pas que le PDF soit un Factur-X valide : lisez un 4xx au premier import comme une réponse de Pennylane à examiner.

## Logs et confidentialité

- Les logs sortent **uniquement sur stderr** — jamais sur stdout, ce qui corromprait le framing MCP du transport stdio.
- Le token Pennylane Bearer est **toujours redacté** via `redact_bearer()` (`Bearer abcd***wxyz`), même en niveau TRACE.
- Les niveaux INFO / WARN / ERROR **n'incluent jamais les bodies de réponse** — la donnée comptable est sensible (RGPD, secret des affaires).
- TRACE expose les URLs de requête complètes et les bodies de réponse. À n'activer qu'en debug local sur un compte sandbox.
- Aucune télémétrie, aucun phone-home, aucune instrumentation opt-in.

## Transport HTTP streamable

```bash
mcp-pennylane --transport http --host 127.0.0.1 --port 8000
# Endpoint MCP : http://127.0.0.1:8000/mcp
# Health        : http://127.0.0.1:8000/health
```

Le token Pennylane reste dans `PENNYLANE_API_KEY` côté serveur — l'instance est mono-tenant par processus. **L'authentification entre le client MCP et ce serveur est volontairement absente.** Bindez sur `127.0.0.1` et placez un reverse proxy devant pour toute exposition non locale (Cloudflare Access, Authelia, Caddy basic-auth, etc.). Un serveur OSS standalone ne doit pas figer d'opinions sur l'auth externe.

**N'exposez ce transport hors de `127.0.0.1` que derrière un reverse proxy qui authentifie.** Depuis l'envoi de fichiers, un appel non authentifié peut faire lire au serveur l'un de ses propres fichiers (PDF ou image) et l'envoyer dans le dossier Pennylane (voir [Envoi de fichiers](#envoi-de-fichiers)).

## Sandbox

`PENNYLANE_ENV=sandbox` est un **repère visuel uniquement** — Pennylane utilise la même URL de base pour le sandbox et la production. La valeur est exposée à trois endroits pour éviter le classique « oups, j'ai modifié la prod » :

1. La bannière de démarrage sur stderr : `mcp-pennylane v0.1.0 — Pennylane Company API v2.0 — env=sandbox — mode=readonly (auto)`.
2. La réponse `getMe`, augmentée de `_mcp_pennylane = { env, server_version, spec_version, readonly, readonly_source, api_2026 }`.
3. Le champ `serverInfo.instructions` du protocole MCP.

## Breaking changes API 2026

Pennylane déploie des breaking changes sur son Company API le **8 avril 2026**. Du 14 janvier au 8 avril 2026, le nouveau comportement est opt-in via `X-Use-2026-API-Changes: true`. Définissez `PENNYLANE_API_2026=true` (ou `--api-2026`) pour envoyer l'en-tête dès maintenant.

| Phase | Quand | Comportement |
|---|---|---|
| Preview | 14 janvier → 8 avril 2026 | Opt-in via la variable d'env |
| Bascule | 8 avril 2026 | Le nouveau comportement devient la valeur upstream par défaut. **v1.0** taggué ce jour-là, la variable devient opt-out / legacy |
| Nettoyage | 1er juillet 2026 | L'ancien comportement est retiré upstream — la variable devient un no-op |

## Catalogue d'outils

Environ 74 outils essentiels sont exposés directement : clients, fournisseurs, factures clients + fournisseurs, produits, devis, banque, journaux, comptes/écritures/lignes du grand livre, balance de vérification, exercices fiscaux, catégories analytiques, exports FEC, pièces jointes, changelogs, mandats GoCardless et SEPA, `getMe`. La totalité des ~163 opérations reste accessible via les méta-outils.

Les noms des outils correspondent à l'`operationId` Pennylane à l'identique (par ex. `getCustomerInvoices`, `postLedgerEntries`), pour grepper proprement contre la spec OpenAPI officielle. La liste curée vit dans la [constante `ESSENTIALS` de `server/build.rs`](./server/build.rs).

## Roadmap

- API Firm Pennylane comme binaire frère `mcp-pennylane-firm`
- Image Docker quand l'usage du transport HTTP le justifiera

## Développement

```bash
# Build
cargo build --release

# Rafraîchir la spec OpenAPI Pennylane vendorisée
cargo run -p refresh-openapi
cargo run -p refresh-openapi -- --diff   # dry run
cargo run -p refresh-openapi -- --check  # exit non-zero en cas de drift

# Suite de tests (unit + invariant)
cargo test --workspace

# Tests d'intégration contre un sandbox Pennylane (lecture seule)
PENNYLANE_API_KEY=… PENNYLANE_READONLY=true cargo test --workspace --features integration-tests

# Sonde d'envoi réel (un seul upload, ignoré par défaut ; token avec le scope file_attachments:all)
PENNYLANE_API_KEY=… cargo test -p mcp-pennylane --features integration-tests --test integration_upload_probe -- --ignored
```

Une GitHub Action hebdomadaire lance `cargo run -p refresh-openapi` chaque lundi à 06h00 UTC et ouvre une PR si la spec upstream a dérivé. La CI sur la PR vérifie que toutes les opérations essentielles sont toujours présentes (`build.rs` panique si une op renommée/retirée casse le contrat) ; mergez quand le vert est là.

## Licence

MIT — voir [LICENSE](LICENSE).
