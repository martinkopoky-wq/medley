# Medley product website

A German product page for the native Medley desktop app. The visual direction takes its cues from record sleeves: warm paper, dark type, vermilion, a vinyl illustration, and original app screenshots. Mix previews, screenshot tabs, installation tabs, clipboard controls and mobile navigation work in the browser. The preview does not stream music.

The site is built with React and Vinext. Fonts and product screenshots are served locally. There is no analytics integration or third-party font request.

## Run the website locally

Requires Node.js 22.13 or newer and npm.

    git clone https://github.com/martinkopoky-wq/medley.git
    cd medley/website
    npm ci
    npm run dev -- --host 127.0.0.1 --port 4173

Open http://127.0.0.1:4173.

Build and preview the production Worker locally:

    cd /path/to/medley/website
    npm run build
    npm run start

The last command prints its local URL. Build outputs live in website/dist/; dependencies live in website/node_modules/. Both are ignored by Git. The tracked .openai/hosting.json retains the Sites project identity used for hosting. Source and deployment are synchronized with the Sites workflow helpers; the page can be developed without those helpers.

If npm forwards a global allow-scripts policy into nested install scripts, use:

    npm run install:ci

The installer preserves the user's npmrc and uses the explicit per-project lifecycle-script allowlist.

## Desktop installation

[INSTALL.md](INSTALL.md) contains complete Linux, macOS, Windows, and source-build commands, with their installation paths and prerequisites. The page imports the same command data from lib/install-data.ts. Update RELEASE_VERSION there when publishing a newer desktop release.

The desktop binary and this website are separate programs. Running the website does not install the music player.

## Spotify compatibility

The desktop app requires Premium for Spotify playback. Web API access and native playback use separate authorizations. Spotify currently refuses audio keys for some accounts even after both logins succeed; [upstream issue #1649](https://github.com/librespot-org/librespot/issues/1649) records the known limitation. The website explains this alongside the install instructions.

## Design attribution

The design process uses [Unslop by Damir Sokolovsky](https://heydamir.design/unslop), licensed under CC BY 4.0. Attribution is retained in the page footer. App screenshots contain demo data; no personal client ID or credentials are included.
