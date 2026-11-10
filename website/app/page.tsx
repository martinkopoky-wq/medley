"use client";

import { memo, useEffect, useState, useSyncExternalStore, type KeyboardEvent } from "react";
import { ArrowDown, ArrowRight, ArrowUpRight, Check, ChevronDown, Copy, CornerDownRight, GitFork, Menu, Shuffle, X } from "lucide-react";
import { INSTALL_DATA, INSTALL_OPTIONS, RELEASE_URL, RELEASE_VERSION, REPOSITORY, type InstallPlatform } from "../lib/install-data";

const mixes = [
  { name: "After hours", subtitle: "Für die letzte Bahn und den langen Heimweg", tracks: [
    { title: "Nightcall", artist: "Kavinsky", service: "Spotify", source: "spotify", duration: "4:18" },
    { title: "Rainy Window", artist: "nightdrive", service: "SoundCloud", source: "soundcloud", duration: "5:12" },
    { title: "A Real Hero", artist: "College & Electric Youth", service: "Apple Music", source: "apple", duration: "0:30" },
  ] },
  { name: "Slow Sunday", subtitle: "Kaffee aufsetzen. Den Rest später machen.", tracks: [
    { title: "Tadow", artist: "Masego & FKJ", service: "Spotify", source: "spotify", duration: "5:01" },
    { title: "Neon Rain", artist: "lofi.kid", service: "SoundCloud", source: "soundcloud", duration: "2:56" },
    { title: "Instant Crush", artist: "Daft Punk", service: "Apple Music", source: "apple", duration: "0:30" },
  ] },
  { name: "City lights", subtitle: "Ein bisschen Nacht passt in jeden Tag.", tracks: [
    { title: "Midnight City", artist: "M83", service: "Spotify", source: "spotify", duration: "4:04" },
    { title: "City Lights at 3AM", artist: "Ambient Drives", service: "SoundCloud", source: "soundcloud", duration: "6:42" },
    { title: "Intro", artist: "The xx", service: "Apple Music", source: "apple", duration: "0:30" },
  ] },
];
const views = [
  { id: "dark", label: "Classic · Dunkel", image: "/screenshots/dark", description: "Bibliothek, Mix und Warteschlange in einer Ansicht" },
  { id: "light", label: "Classic · Hell", image: "/screenshots/light", description: "Die gleiche Musik, mit etwas mehr Tageslicht" },
  { id: "dj-booth", label: "DJ Booth", image: "/screenshots/dj-booth", description: "Mehr Raum für den nächsten Track" },
  { id: "ai-studio", label: "AI Studio", image: "/screenshots/ai-studio", description: "Von einer Stimmung zu einem Mix mit echten Songs" },
] as const;

function tabKeys<T extends string>(event: KeyboardEvent, options: readonly T[], selected: T, set: (value: T) => void, prefix: string) {
  let index = options.indexOf(selected);
  if (event.key === "ArrowRight") index = (index + 1) % options.length;
  else if (event.key === "ArrowLeft") index = (index - 1 + options.length) % options.length;
  else if (event.key === "Home") index = 0;
  else if (event.key === "End") index = options.length - 1;
  else return;
  event.preventDefault();
  set(options[index]);
  document.getElementById(prefix + "-" + options[index])?.focus();
}

function CopyCommand({ command }: { command: string }) {
  const [status, setStatus] = useState<"idle" | "copied" | "error">("idle");
  useEffect(() => {
    if (status === "idle") return;
    const timer = window.setTimeout(() => setStatus("idle"), 2400);
    return () => window.clearTimeout(timer);
  }, [status]);
  return <button className="copy-button" onClick={async () => {
    try { await navigator.clipboard.writeText(command); setStatus("copied"); }
    catch { setStatus("error"); }
  }} aria-label={status === "copied" ? "Befehl kopiert" : "Befehl kopieren"}>
    {status === "copied" ? <Check size={16} /> : <Copy size={16} />}
    <span aria-live="polite">{status === "copied" ? "Kopiert" : status === "error" ? "Bitte Text markieren" : "Kopieren"}</span>
  </button>;
}

const Header = memo(function Header() {
  const [menuOpen, setMenuOpen] = useState(false);
  return <header className="site-header wrap">
    <a className="brand" href="#" aria-label="Medley Startseite"><img src="/icon.png" width="32" height="32" alt="" /><span>medley<span className="brand-period">.</span></span></a>
    <nav aria-label="Hauptnavigation" className={menuOpen ? "navigation open" : "navigation"}>
      <a href="#idee" onClick={() => setMenuOpen(false)}>Die Idee</a><a href="#einblicke" onClick={() => setMenuOpen(false)}>Einblicke</a>
      <a className="nav-github" href={REPOSITORY} target="_blank" rel="noreferrer">GitHub <ArrowUpRight size={16} /></a>
    </nav>
    <a className="button button-small header-download" href="#installieren">Medley holen <ArrowDown size={16} /></a>
    <button className="menu-button" aria-label={menuOpen ? "Menü schließen" : "Menü öffnen"} aria-expanded={menuOpen} onClick={() => setMenuOpen(!menuOpen)}>{menuOpen ? <X size={24} /> : <Menu size={24} />}</button>
  </header>;
});

const Hero = memo(function Hero() {
  const [mixIndex, setMixIndex] = useState(0);
  const mix = mixes[mixIndex];
  return <section className="hero wrap" aria-labelledby="hero-title">
    <div className="hero-copy">
      <p className="eyebrow"><span className="tiny-record" aria-hidden="true" /> Ein Zuhause für deinen Musikgeschmack</p>
      <h1 id="hero-title">Deine Musik.<br />Spielt<br /><span className="accent-word">zusammen.</span></h1>
      <p className="hero-description">Spotify, SoundCloud und Apple Music.<br className="desktop-break" /> Deine Lieblingssongs in einer nativen App.<br className="desktop-break" /> Und endlich in demselben Mix.</p>
      <div className="hero-actions"><a className="button" href="#installieren">Jetzt Medley holen <ArrowDown size={18} /></a><a className="text-link" href="#einblicke">App ansehen <ArrowUpRight size={18} /></a></div>
      <p className="hero-meta">Open Source in Rust <span>·</span> Linux, macOS & Windows</p>
    </div>
    <div className="hero-art" aria-label="Interaktive Beispielmix-Vorschau">
      <div className="record-heading"><span>Selected by you</span><span>Vol. 0{mixIndex + 1}</span></div>
      <div className="record-scene">
        <img className="scene-img" src="/scene/sleeve-vinyl.webp" srcSet="/scene/sleeve-vinyl-768.webp 768w, /scene/sleeve-vinyl.webp 1536w" sizes="(max-width: 620px) calc(100vw - 36px), (max-width: 1100px) calc(50vw - 60px), 560px" width="1536" height="1024" alt="Sage-grünes Plattencover auf Papier, aus dem eine schwarze Vinyl-Schallplatte mit rotem Label geschoben wird" fetchPriority="high" />
        <div className="sleeve-mark" aria-hidden="true"><span>medley.</span><small>One library.<br />Every side of you.</small></div>
        <div className="record-sticker" aria-hidden="true">3 Dienste<br /><strong>1 Mix.</strong></div>
      </div>
      <div className="mix-preview">
        <div className="mix-heading"><div><p>Dein nächster Lieblingsmix</p><h2>{mix.name}</h2></div><button className="round-button" onClick={() => setMixIndex((mixIndex + 1) % mixes.length)} aria-label="Anderen Beispielmix anzeigen" title="Beispielmix wechseln"><Shuffle size={20} /></button></div>
        <p className="mix-description">{mix.subtitle}</p>
        <ol className="mix-tracks" aria-live="polite">{mix.tracks.map((track, index) => <li key={track.title}>
          <span className="track-index">0{index + 1}</span><span className={"track-cover cover-" + index} aria-hidden="true"><span /></span>
          <div className="track-info"><strong>{track.title}</strong><span>{track.artist}</span></div>
          <span className={"source-dot " + track.source} title={track.service}><span className="sr-only">{track.service}</span></span><span className="track-duration">{track.duration}</span>
        </li>)}</ol>
        <div className="mix-legend"><span><i className="source-dot spotify" />Spotify</span><span><i className="source-dot soundcloud" />SoundCloud</span><span><i className="source-dot apple" />Apple Music</span></div>
      </div>
      <p className="art-caption">Interaktive Vorschau · Apple Music spielt 30-Sekunden-Previews</p>
    </div>
  </section>;
});

const ServiceLine = memo(function ServiceLine() {
  return <div className="service-line wrap"><span>Für die Songs, die du schon liebst.</span><div><strong><i className="source-dot spotify" />Spotify</strong><strong><i className="source-dot soundcloud" />SoundCloud</strong><strong><i className="source-dot apple" />Apple Music</strong></div></div>;
});

const IdeaSection = memo(function IdeaSection() {
  return <section id="idee" className="idea-section wrap section-space" aria-labelledby="idea-title">
    <div className="section-intro"><p className="kicker">Die Idee</p><h2 id="idea-title">Musikgeschmack<br />hält sich an<br /><span className="serif-i">deine Regeln.</span></h2><p>Ein Track auf Spotify. Ein Set auf SoundCloud. Die Playlist bei Apple Music. Medley bringt zusammen, was für dich zusammengehört.</p><a className="text-link" href="#installieren">Bring deine Sammlung mit <ArrowRight size={18} /></a></div>
    <div className="idea-steps">
      <article><span className="step-number">01</span><div><h3>Einmal suchen. Überall finden.</h3><p>Eine Suche für deine verbundenen Dienste. Bibliotheken, Likes und Playlists liegen direkt nebeneinander.</p><div className="search-example"><span>Nightcall</span><span className="search-sources"><i className="source-dot spotify" /><i className="source-dot soundcloud" /><i className="source-dot apple" /></span><kbd className="keycap" aria-hidden="true">↵</kbd></div></div></article>
      <article><span className="step-number">02</span><div><h3>Dein Mix kennt alle Seiten.</h3><p>Pack Songs verschiedener Dienste in einen Medley-Mix. Jeder Track spielt aus der Quelle, aus der du ihn hinzugefügt hast.</p></div></article>
      <article><span className="step-number">03</span><div><h3>Nimm die Reihenfolge mit.</h3><p>Exportiere einen Mix als echte Playlist zu deinem Dienst. Medley sucht passende Versionen und zeigt dir, was fehlt.</p></div></article>
    </div>
  </section>;
});

const Showcase = memo(function Showcase() {
  const [view, setView] = useState<(typeof views)[number]["id"]>("dark");
  const screenshot = views.find((item) => item.id === view)!;
  return <section id="einblicke" className="showcase section-space" aria-labelledby="showcase-title">
    <div className="wrap"><div className="showcase-heading"><div><p className="kicker">Ansichten</p><h2 id="showcase-title">So, wie du hörst.</h2></div><p>Panels verschieben, Ansichten wechseln, deinen Platz finden. Die Oberfläche passt sich deinem Rhythmus an.</p></div>
      <div className="view-tabs" role="tablist" aria-label="App-Ansicht" onKeyDown={(event) => tabKeys(event, views.map((item) => item.id), view, setView, "view")}>{views.map((item) => <button key={item.id} id={"view-" + item.id} role="tab" aria-selected={view === item.id} tabIndex={view === item.id ? 0 : -1} aria-controls="view-panel" onClick={() => setView(item.id)}>{item.label}</button>)}</div>
      <div className="product-window" id="view-panel" role="tabpanel" aria-labelledby={"view-" + view}><div className="window-bar"><span className="window-controls" aria-hidden="true"><i /><i /><i /></span><span>Medley / {screenshot.label}</span><a href={screenshot.image + ".png"} target="_blank" rel="noreferrer" aria-label="Original-Screenshot in voller Größe öffnen"><ArrowUpRight size={16} /></a></div><div className="screen-viewport"><img src={screenshot.image + ".webp"} srcSet={`${screenshot.image}-640.webp 640w, ${screenshot.image}.webp 941w`} sizes="(max-width: 620px) calc(100vw - 40px), (max-width: 1100px) calc(100vw - 64px), 1184px" alt={"Original-Screenshot von Medley: " + screenshot.description} width="941" height="1022" loading="lazy" decoding="async" /></div></div>
      <div className="screenshot-caption"><span>{screenshot.description}</span><span>Original-Screenshots · Demodaten</span></div>
    </div>
  </section>;
});

const NativeSection = memo(function NativeSection() {
  return <section className="native-section wrap section-space" aria-labelledby="native-title"><div className="native-copy"><p className="kicker">Unter der Haube</p><h2 id="native-title">Für deinen Desktop.<br /><span className="serif-i">Für deinen Kopf.</span></h2><p>Medley ist eine native Rust-App mit einer eigenen Oberfläche. Die Audioausgabe wird bei Inaktivität freigegeben, Streams werden dateibasiert gepuffert.</p></div><div className="native-details"><article><span className="detail-side">Seite A</span><div><h3>Ein Vibe ist ein Anfang.</h3><p>Beschreibe eine Stimmung und lass einen Mix mit echten Songs entstehen. Optional lokal mit Ollama oder über Claude.</p><blockquote className="prompt-example">„Regen am Fenster. Jazz, aber wach bleiben.“<small>AI Studio · Vibe zu Mix</small></blockquote></div></article><article><span className="detail-side">Seite B</span><div><h3>Deine Dienste bleiben deine Dienste.</h3><p>Du meldest dich direkt im Browser an. Medley verbindet sich mit deinen eigenen Konten und speichert deine Mixes und Layouts auf deinem Rechner.</p></div></article><a href={REPOSITORY} target="_blank" rel="noreferrer" className="source-link"><GitFork size={20} /><span>Offener Code. Eigene Entscheidungen.</span><ArrowUpRight size={18} /></a></div></section>;
});

// The OS tab follows the visitor's user agent. useSyncExternalStore keeps the
// server render on Linux and switches after hydration without a setState effect.
const subscribeNothing = () => () => {};
const platformFromUserAgent = (): InstallPlatform => {
  if (/Windows/i.test(navigator.userAgent)) return "windows";
  if (/Macintosh|Mac OS X/i.test(navigator.userAgent) && !/iPhone|iPad/i.test(navigator.userAgent)) return "macos";
  return "linux";
};
const platformFromServer = (): InstallPlatform => "linux";

const InstallSection = memo(function InstallSection() {
  const detected = useSyncExternalStore(subscribeNothing, platformFromUserAgent, platformFromServer);
  const [override, setOverride] = useState<InstallPlatform | null>(null);
  const platform = override ?? detected;
  const install = INSTALL_DATA[platform];
  return <section id="installieren" className="install-section section-space" aria-labelledby="install-title"><div className="wrap"><div className="install-heading"><div><p className="kicker">Installieren · v{RELEASE_VERSION}</p><h2 id="install-title">Ein Befehl.<br /><span className="serif-i">Gleich läuft’s.</span></h2></div><a className="release-link" href={RELEASE_URL} target="_blank" rel="noreferrer">Release v{RELEASE_VERSION}<ArrowUpRight size={16} /></a></div>
    <div className="installation"><aside className="install-sidebar"><div className="platform-tabs" role="tablist" aria-label="Betriebssystem" onKeyDown={(event) => tabKeys(event, INSTALL_OPTIONS.map((item) => item.id), platform, setOverride, "platform")}>{INSTALL_OPTIONS.map((item) => <button key={item.id} id={"platform-" + item.id} role="tab" aria-selected={platform === item.id} aria-controls="install-panel" tabIndex={platform === item.id ? 0 : -1} onClick={() => setOverride(item.id)}>{item.label}<ArrowUpRight size={16} /></button>)}</div><div className="install-location"><span>Wird installiert unter</span><code>{install.installPath}</code></div><div className="install-formats">{install.formats.map((format) => <a key={format.url} className="install-format" href={format.url} target="_blank" rel="noreferrer"><ArrowDown size={16} /><span><strong>{format.label}</strong><small>{format.note}</small></span></a>)}</div><span className="architecture">{install.architecture}</span></aside>
      <div className="install-main" id="install-panel" role="tabpanel" aria-labelledby={"platform-" + platform}><div className="command-heading"><span><i />{install.shell}</span><CopyCommand command={install.command} /></div><pre className="command-block"><code>{install.command}</code></pre><p className="requirement">{install.requirements}</p><ul className="install-notes">{install.notes.map((note) => <li key={note}>{note}</li>)}</ul>
        {install.fallbackCommand ? <details className="install-extra"><summary>{install.fallbackLabel}<ChevronDown size={16} /></summary><div className="extra-command"><pre><code>{install.fallbackCommand}</code></pre><CopyCommand command={install.fallbackCommand} /></div></details> : null}
        {install.prerequisites?.map((item) => <details className="install-extra" key={item.label}><summary>{item.label}<ChevronDown size={16} /></summary><div className="extra-command"><pre><code>{item.command}</code></pre><CopyCommand command={item.command} /></div></details>)}
      </div></div>
      <div className="first-start"><CornerDownRight className="first-start-icon" size={20} aria-hidden="true" /><div><h3>Danach: Konten verbinden. Lieblingssong an.</h3><p>Öffne <strong>Accounts</strong> in Medley. Spotify braucht Premium für die Wiedergabe und eine separate Browserfreigabe beim ersten Abspielen. Bei API-Limits kannst du deine eigene Client-ID eintragen. SoundCloud braucht App-Zugangsdaten, Apple Music einen MusicKit-Schlüssel und spielt Previews. KI ist optional.</p><a className="text-link" href={REPOSITORY + "#connecting-your-services"} target="_blank" rel="noreferrer">So verbindest du deine Dienste <ArrowUpRight size={16} /></a><p className="compatibility-note">Spotify-Hinweis: Einige Konten erhalten trotz Premium derzeit keine Audiofreigabe über librespot. Suche und Bibliothek können weiterhin funktionieren. <a href="https://github.com/librespot-org/librespot/issues/1649" target="_blank" rel="noreferrer">Aktueller Kompatibilitätsstand <ArrowUpRight size={13} /></a></p></div></div>
  </div></section>;
});

const FaqSection = memo(function FaqSection() {
  return <section className="faq-section wrap section-space" aria-labelledby="faq-title"><div><p className="section-number">Noch eine Frage?</p><h2 id="faq-title">Gut zu wissen.</h2></div><div className="faq-list"><details><summary>Kann ich ganze Songs abspielen?<ChevronDown size={20} /></summary><p>Spotify benötigt Premium. Manche Konten erhalten derzeit auch nach erfolgreicher Anmeldung keine Audiofreigabe; dann ist die offizielle Spotify-App für die Wiedergabe nötig. Auf SoundCloud hängt es von der Freigabe des Uploaders ab. Apple Music spielt in Medley 30-Sekunden-Previews; für ganze Songs öffnest du Apple Music.</p></details><details><summary>Muss ich KI einrichten?<ChevronDown size={20} /></summary><p>Du kannst Medley ohne KI verwenden. Für die optionalen Funktionen wählst du in Settings Ollama, Claude oder Off. Ollama läuft lokal auf deinem Rechner.</p></details><details><summary>Wo liegen meine Daten?<ChevronDown size={20} /></summary><p>Medley speichert Einstellungen, Mixes und Layouts lokal. Unter Linux liegen Einstellungen in ~/.config/medley/ und Mixes in ~/.local/share/medley/. Anmeldetokens werden lokal als JSON mit geschützten Dateirechten gespeichert.</p></details><details><summary>Ist Medley unabhängig von den Musikdiensten?<ChevronDown size={20} /></summary><p>Ja. Medley ist ein unabhängiges Open-Source-Projekt und gehört weder zu Spotify noch zu SoundCloud oder Apple. Du brauchst eigene Konten und gegebenenfalls eigene Entwickler-Zugangsdaten.</p></details></div></section>;
});

const Footer = memo(function Footer() {
  return <footer className="site-footer"><div className="wrap"><div className="footer-top"><span>Für alle Seiten deines Musikgeschmacks.</span><a href="#installieren">Hol dir Medley <ArrowUpRight size={20} /></a></div><div className="footer-wordmark" aria-hidden="true">medley<span>.</span></div><div className="footer-bottom"><span>Unabhängig. Open Source. In Rust gebaut.</span><div><a href={REPOSITORY} target="_blank" rel="noreferrer">GitHub</a><a href={RELEASE_URL} target="_blank" rel="noreferrer">Releases</a><a href={REPOSITORY + "/blob/main/LICENSE-MIT"} target="_blank" rel="noreferrer">Lizenz</a></div></div><p className="design-credit">Gestaltung mit <a href="https://heydamir.design/unslop" target="_blank" rel="noreferrer">Unslop von Damir Sokolovsky</a> · CC BY 4.0</p></div></footer>;
});

export default function Home() {
  return <>
    <a className="skip-link" href="#main">Zum Inhalt</a>
    <Header />
    <main id="main">
      <Hero />
      <ServiceLine />
      <IdeaSection />
      <Showcase />
      <NativeSection />
      <InstallSection />
      <FaqSection />
    </main>
    <Footer />
  </>;
}
