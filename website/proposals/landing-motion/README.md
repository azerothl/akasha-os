# Landing page motion proposal

This is a standalone, French-language exploration of the Akasha OS homepage. It is served from this folder and does not replace `website/index.html`.

The six scroll-led scenes present the Preview as an operating system: a cobalt boot sequence, a shared Chat / Create / Memory / Caps rail, a local chat, a Create job, capability boundaries, and an installation path. The interface is rebuilt in HTML/CSS and React; its product examples are illustrative.

The visual pacing takes inspiration from the [Cold Club Scrolltide reference](https://www.scrolltide.co/templates/cold-club). No Scrolltide assets are included.

## Preview

After the GitHub Pages site is published, open [the proposal](https://azerothl.github.io/akasha-os/proposals/landing-motion/).

## Edit and rebuild

```powershell
cd source
npm install
npm run dev
```

The development server opens at `http://localhost:5173/`. Run `npm run build` to regenerate the static preview in this folder. The build uses relative asset paths so it works from the nested Pages URL.

Content follows the existing homepage; links go to the official Akasha OS pages. The application label reflects its current Preview version.

