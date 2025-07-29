---
title: "Set up"
date: 2025-03-13T07:00:00Z
aliases:
  - /authoring/install
---

home is available via [homebrew](https://brew.sh), from the [bearcove tap](https://github.com/bearcove/tap):

## Installing from homebrew

Add the tap:

```bash
brew tap bearcove/tap
```

Install home:

```bash
brew install bearcove/tap/home
```

Make sure it works:

```bash
home doctor
```

Does it?

```term
<i class="b fg-cyn">home.bearcove.eu</i> on <i class="b fg-mag"> main</i> <i class="b fg-red">[$!⇡]</i>
<i class="b fg-grn">❯</i> <i class="fg-blu">home </i><i class="fg-cyn">doctor</i>
<i class="fg-grn">All checks passed successfully</i>
```

Then welcome home!

## Keeping home updated

When a new version of home comes out, it's tagged on <https://github.com/bearcove/home/releases>

Shortly after, a [CI workflow / action](https://github.com/bearcove/home/actions) updates
the [bearcove tap](https://github.com/bearcove/homebrew-tap) repository — its commit list shows
that it's all happening automatically:

+++
:figure:
    src: bearcove-tap-commits@2x.jxl
    title: |
        Automation goes brrrr
    alt: |
        A screenshot of the GitHub bearcove/tap repository showing commits made a few minutes distant from each other.
+++

## Serving an existing home site

Running `home serve --open` in an existing home site should work out of the box.

It will:

  * Assume the name of the current folder is the domain of the tenant
    (that's why repositories are named things like `fasterthanli.me`
    and `home.bearcove.eu`)
  * Look for an environment variable named `HOME_API_KEY` — if it doesn't
    find one, it'll still start up, but you won't be able to deploy.

```term
<i class="b fg-cyn">home.bearcove.eu</i> on <i class="b fg-mag"> main</i> <i class="b fg-red">[!?]</i>
<i class="b fg-grn">❯</i> <i class="fg-blu">home </i><i class="fg-cyn">serve --open</i>
Reading .env file from: /Users/amos/.env
Reading .env file from: /Users/amos/bearcove/home.bearcove.eu/.env
Config-less mode enabled for tenant home.bearcove.eu
<i class="fg-grn"> INFO</i> <i class="l">mod_revision::impls::load:</i> Built <i class="fg-grn">7</i>/<i class="fg-ylw">7</i> pages in <i class="fg-cyn">3.985875ms</i>
<i class="fg-grn"> INFO</i> <i class="l">mod_cub::impls:</i> Will serve <i class="fg-cyn">rev_01JQNW09MP7E7DMM77KPRBCZ06</i> (loaded in <i class="fg-ylw">0ns</i>)
<i class="fg-grn"> INFO</i> <i class="l">mod_revision::impls::watch:</i> [<i class="fg-mag">home.bearcove.eu</i>] Watching <i class="fg-ylw">/Users/amos/bearcove/home.bearcove.eu/content/</i>
<i class="fg-grn"> INFO</i> <i class="l">mod_revision::impls::watch:</i> [<i class="fg-mag">home.bearcove.eu</i>] Watching <i class="fg-ylw">/Users/amos/bearcove/home.bearcove.eu/templates/</i>
<i class="fg-grn"> INFO</i> <i class="l">mod_cub::impls:</i> 🦊 Visit the site at <i class="fg-blu">http://home.bearcove.eu.snug.blog:1111</i>
<i class="fg-grn"> INFO</i> <i class="l">mod_cub::impls:</i> <i class="fg-cyn">GET</i> <i class="fg-ylw">/</i> -&gt; <i class="fg-mag">200</i> (took 300.917µs)
✂️
```

> You don't _have_ to use `--open`, but.. you can.

## Setting up a home site

In a new folder...

```term
<i class="b fg-cyn">~/bearcove</i>
<i class="b fg-grn">❯</i> <i class="fg-blu">mkdir </i><i class="fg-cyn">home-from-scratch</i>

<i class="b fg-cyn">~/bearcove</i>
<i class="b fg-grn">❯</i> <i class="b u fg-blu">cd </i><i class="b u fg-cyn">home-from-scratch/</i>
```

The `home init` command sets up everything:

```term
<i class="b fg-cyn">~/bearcove/home-from-scratch</i>
<i class="b fg-grn">❯</i> <i class="fg-blu">home </i><i class="fg-cyn">init</i>
<i class="fg-blu">📋 The following files will be created:</i>
  <i class="fg-cyn">home.json</i>
  <i class="fg-cyn">content/_index.md</i>
  <i class="fg-cyn">templates/page.html.jinja</i>
  <i class="fg-cyn">src/bundle.ts</i>
  <i class="fg-cyn">src/main.scss</i>
  <i class="fg-cyn">src/_reset.scss</i>

<i class="fg-grn">Do you want to proceed? (y/N): </i>y
```

Answering `y` will create the files for you:

```term
📄 Created file: <i class="fg-cyn">./home.json</i>
📄 Created file: <i class="fg-cyn">./content/_index.md</i>
📄 Created file: <i class="fg-cyn">./templates/page.html.jinja</i>
📄 Created file: <i class="fg-cyn">./src/bundle.ts</i>
📄 Created file: <i class="fg-cyn">./src/main.scss</i>
📄 Created file: <i class="fg-cyn">./src/_reset.scss</i>
<i class="fg-grn">✨ Created initial content and source files! 🎉</i>
📦 <i class="fg-ylw">package.json</i> not found. Running <i class="fg-cyn">&#96;pnpm init&#96;</i> to create it...
✅ Successfully created <i class="fg-ylw">package.json</i>
🔄 Updated <i class="fg-ylw">package.json</i> to set <i class="fg-cyn">"type": "module"</i>
📝 Updated <i class="fg-ylw">.gitignore</i> with required entries
<i class="fg-grn">🚀 Development setup completed successfully! 🎊</i>

<i class="fg-ylw">=== 🌟 You're all set! 🌟 ===</i>
<i class="fg-blu">📌 Next step:</i> Run <i class="fg-cyn">&#96;home serve&#96;</i> to start the development server.
<i class="fg-grn">🎈 Happy coding! 🎈</i>
```

And then you can run it with `home serve`:

```term
<i class="b fg-cyn">~/bearcove/home-from-scratch</i>
<i class="b fg-grn">❯</i> <i class="fg-blu">home </i><i class="fg-cyn">serve</i>
Reading config from env (HOMECONF_ prefix) and <i class="fg-grn">./home.json</i>
Absolute config path: <i class="fg-grn">/Users/amos/bearcove/home-from-scratch/home.json</i>
Resolved base_dir for tenant home-from-scratch: <i class="fg-grn">/Users/amos/bearcove/home-from-scratch</i>
<i class="fg-grn"> INFO</i> <i class="l">mod_revision::impls::make:</i> Processed <i class="fg-ylw">4</i> events in <i class="fg-cyn">181.208µs</i> (<i class="fg-grn">4</i> add actions)
<i class="fg-grn"> INFO</i> <i class="l">mod_revision::impls::load:</i> Built <i class="fg-grn">1</i>/<i class="fg-ylw">1</i> pages in <i class="fg-cyn">1.096ms</i>
<i class="fg-grn"> INFO</i> <i class="l">mod_cub::impls:</i> Will serve <i class="fg-cyn">rev_01JQGSK1PKMS2NY92C8J9B6EA3</i> (loaded in <i class="fg-ylw">42ns</i>)
<i class="fg-grn"> INFO</i> <i class="l">mod_revision::impls::watch:</i> [<i class="fg-mag">home-from-scratch</i>] Watching <i class="fg-ylw">/Users/amos/bearcove/home-from-scratch/content</i>
<i class="fg-grn"> INFO</i> <i class="l">mod_cub::impls:</i> 🦊 http://home-from-scratch.snug.blog:1111
<i class="fg-ylw"> WARN</i> <i class="l">mod_cub::impls::cub_req:</i> No tenant found for domain cdn.home.bearcove.eu.snug.blog
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls::db::migrations:</i> Applying migration "m0001_initial"
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls::db::migrations:</i> Applying migration "m0003_patreon_credentials"
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls::db::migrations:</i> Applying migration "m0004_github_credentials"
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls::db::migrations:</i> Applying migration "m0005_create_sponsor_table"
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls::db::migrations:</i> Applying migration "m0006_create_revisions_table"
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls::db::migrations:</i> Applying migration "m0007_create_objectstore_entries_table"
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls::db::migrations:</i> Applying migration "m0008_objectstore_entries_rename"
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls:</i> No revisions found in database
<i class="fg-grn"> INFO</i> <i class="l">mod_mom::impls:</i> 🐻 mom is now serving on 127.0.0.1:49941 💅
<i class="fg-ylw"> WARN</i> <i class="l">mod_cub::impls::cub_req:</i> No tenant found for domain home.bearcove.eu.snug.blog
<i class="fg-grn"> INFO</i> <i class="l">mod_cub::impls:</i> <i class="fg-cyn">GET</i> <i class="fg-ylw">/internal-api/ws</i> -&gt; <i class="fg-mag">400</i> (took 647.708µs)
<i class="fg-ylw"> WARN</i> <i class="l">mod_cub::impls::cub_req:</i> No tenant found for domain cdn.home.bearcove.eu.snug.blog
<i class="fg-ylw"> WARN</i> <i class="l">mod_cub::impls::cub_req:</i> No tenant found for domain cdn.home.bearcove.eu.snug.blog
^C<i class="fg-ylw"> WARN</i> <i class="l">mod_mom::impls::endpoints:</i> Received SIGINT
<i class="fg-ylw"> WARN</i> <i class="l">mod_mom::impls::endpoints:</i> Exiting immediately
<i class="fg-ylw"> WARN</i> <i class="l">mod_cub::impls::graceful_shutdown:</i> Received SIGINT
<i class="fg-ylw"> WARN</i> <i class="l">mod_cub::impls::graceful_shutdown:</i> Exiting immediately
```

If you then open <http://localhost:1111>, you'll see something like this:

+++
:media:
    src: no-tenant-found@2x.jxl
    alt: |
        No tenant found for domain localhost
        Available tenants:
        • home-from-scratch
+++

And if you click `home-from-scratch`, you'll be redirected to
<http://home-from-scratch.localhost:1111>, showing this:

+++
:media:
    src: its-empty-in-here@2x.jxl
    alt: |
        Safari screenshot, showing: It's empty in here — so many possibilities though!
+++
