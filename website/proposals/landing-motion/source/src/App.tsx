import { useRef, useState, type ReactNode, type RefObject } from "react";
import {
  ArrowDown,
  ArrowRight,
  ArrowUpRight,
  Brain,
  ChevronDown,
  Download,
  FileText,
  LockKeyhole,
  Menu,
  MessageCircle,
  Monitor,
  Shield,
  Sparkles,
  WandSparkles,
  WifiOff,
  X,
} from "lucide-react";
import { PinnedScene } from "./landing/PinnedScene";
import { lerp, segment, setStyle, smooth } from "./landing/scroll";

type Surface = "chat" | "create" | "memory" | "caps";
type Scene = "boot" | "desktop" | "chat" | "create" | "caps" | "install";
const base = "https://azerothl.github.io/akasha-os/";
const assetBase = import.meta.env.BASE_URL;
const urls = {
  install: base + "install.html?lang=fr",
  docs: base + "docs/?lang=fr",
  firstRun: base + "docs/first-run.html?lang=fr",
  news: base + "docs/whats-new.html?lang=fr",
  community: base + "community.html?lang=fr",
  about: base + "about.html?lang=fr",
  github: "https://github.com/azerothl/akasha-os",
};
const nav: { id: Surface; name: string; Icon: typeof MessageCircle; description: string }[] = [
  { id: "chat", name: "Chat", Icon: MessageCircle, description: "Multi-sessions. Modèles locaux. Hors ligne par défaut." },
  { id: "create", name: "Create", Icon: WandSparkles, description: "Packs image et vidéo. Recettes du catalogue. Progression des jobs." },
  { id: "memory", name: "Memory", Icon: Brain, description: "Journal de décisions. Remember et recall." },
  { id: "caps", name: "Caps", Icon: Shield, description: "Jetons révocables. Réseau refusé par défaut. Audit." },
];

function Brand() {
  return <span className="os-brand"><img src={`${assetBase}akasha-mark.svg`} alt="" width="34" height="34" /><span>AKASHA<span className="os-brand__os"> OS</span></span></span>;
}

function Header() {
  const [open, setOpen] = useState(false);
  return (
    <header className="os-header">
      <a className="os-header__brand" href="#top" aria-label="Akasha OS, accueil"><Brand /></a>
      <nav className={"os-header__nav" + (open ? " is-open" : "")} aria-label="Navigation principale">
        <a href="#desktop" onClick={() => setOpen(false)}>Le système</a>
        <a href="#create" onClick={() => setOpen(false)}>Create</a>
        <a href="#caps" onClick={() => setOpen(false)}>Caps</a>
        <a href={urls.docs} target="_blank" rel="noreferrer">Docs <ArrowUpRight size={15} /></a>
      </nav>
      <a className="os-header__cta" href={urls.install} target="_blank" rel="noreferrer">Installer la Preview <ArrowUpRight size={16} /></a>
      <button className="os-header__menu" type="button" aria-label={open ? "Fermer le menu" : "Ouvrir le menu"} aria-expanded={open} onClick={() => setOpen(!open)}>
        {open ? <X size={22} /> : <Menu size={22} />}
      </button>
    </header>
  );
}

function ChapterTag({ no, label }: { no: string; label: string }) {
  return <span className="os-chapter-tag"><b>{no}</b><i />{label}</span>;
}

function OsWindow({ surface, children, className = "" }: { surface: Surface; children: ReactNode; className?: string }) {
  return (
    <div className={"os-window " + className} aria-hidden="true">
      <div className="os-window__bar">
        <span className="os-window__dots"><i /><i /><i /></span>
        <span className="os-window__title"><img src={`${assetBase}akasha-mark.svg`} alt="" /> AKASHA / PREVIEW</span>
        <span className="os-window__meta">0.19.0 <span className="os-window__barline" /> LOCAL HOST</span>
      </div>
      <div className="os-window__body">
        <div className="os-window__rail">
          {nav.map(({ id, Icon }) => <span key={id} className={surface === id ? "is-active" : ""}><Icon size={20} strokeWidth={1.7} /></span>)}
        </div>
        <div className="os-window__main">{children}</div>
      </div>
    </div>
  );
}

function ScreenHeader({ label, title, status }: { label: string; title: string; status?: string }) {
  return <div className="os-screen-head"><div><span>{label}</span><strong>{title}</strong></div>{status && <em>{status}</em>}</div>;
}

function ChatPanel({ promptRef, replyRef }: { promptRef?: RefObject<HTMLDivElement | null>; replyRef?: RefObject<HTMLDivElement | null> }) {
  return (
    <div className="os-chat-screen">
      <ScreenHeader label="CHAT / EXEMPLE" title="Théière - atelier" status="LOCAL" />
      <div className="os-chat-screen__thread">
        <div ref={promptRef} className="os-chat-screen__prompt os-moving">Peux-tu m'aider à décrire une théière rouge pour Create ?</div>
        <div ref={replyRef} className="os-chat-screen__reply os-moving"><span>MODÈLE LOCAL</span><p>Une théière rouge, une lumière douce, une table en bois clair. Tu peux lancer la génération dans Create.</p></div>
      </div>
      <div className="os-chat-screen__input">Message ou commande <ArrowRight size={16} /></div>
    </div>
  );
}

function MemoryPanel() {
  return (
    <div className="os-memory-screen">
      <ScreenHeader label="MEMORY / EXEMPLE" title="Journal et mémoire" status="LOCAL" />
      <div className="os-memory-screen__body">
        <div><span>01</span><strong>Journal de décisions</strong><ArrowUpRight size={18} /></div>
        <div><span>02</span><strong>Remember</strong><ArrowUpRight size={18} /></div>
        <div><span>03</span><strong>Recall</strong><ArrowUpRight size={18} /></div>
      </div>
    </div>
  );
}

function CapsPanel() {
  return (
    <div className="os-caps-screen">
      <ScreenHeader label="CAPS / EXEMPLE" title="Capacités et audit" status="BORNÉ" />
      <div className="os-caps-screen__body">
        <div><WifiOff size={20} /><span><strong>Sortie réseau</strong><small>Refusée par défaut</small></span><b>BLOCKED</b></div>
        <div><LockKeyhole size={20} /><span><strong>Jetons de capacité</strong><small>Révocables</small></span><b>REVOCABLE</b></div>
        <div><FileText size={20} /><span><strong>Piste d'audit</strong><small>Actions traçables</small></span><b>LOGGING</b></div>
      </div>
    </div>
  );
}

function TeapotVisual() {
  return (
    <div className="os-teapot" aria-hidden="true">
      <div className="os-teapot__arc" />
      <div className="os-teapot__floor" />
      <div className="os-teapot__cloth" />
      <div className="os-teapot__handle" />
      <div className="os-teapot__spout" />
      <div className="os-teapot__body" />
      <div className="os-teapot__lid" />
      <div className="os-teapot__knob" />
      <span>PREVIEW IMAGE / EXEMPLE</span>
    </div>
  );
}

function CreatePanel({
  recipeRef,
  jobRef,
  resultRef,
  progressRef,
}: {
  recipeRef?: RefObject<HTMLDivElement | null>;
  jobRef?: RefObject<HTMLDivElement | null>;
  resultRef?: RefObject<HTMLDivElement | null>;
  progressRef?: RefObject<HTMLDivElement | null>;
}) {
  return (
    <div className="os-create-screen">
      <ScreenHeader label="CREATE / EXEMPLE" title="Générer une image" status="PACK INSTALLÉ" />
      <div className="os-create-screen__body">
        <div className="os-create-screen__form">
          <div className="os-create-screen__tabs"><b>IMAGE</b><span>VIDÉO</span></div>
          <label><small>PACK IMAGE</small><span>Stable Diffusion 1.5 <ChevronDown size={14} /></span></label>
          <label><small>PROMPT</small><span>Une théière rouge sur une table en bois clair, lumière douce.</span></label>
          <div ref={recipeRef} className="os-create-screen__recipe os-moving">
            <strong><Sparkles size={15} /> RECETTE DU CATALOGUE</strong>
            <span>Taille <i /></span><span>Frames <i /></span><span>Flow-shift <i /></span><span>Offload <i /></span>
          </div>
          <div ref={jobRef} className="os-create-screen__job os-moving"><span>JOB EN COURS</span><i><b ref={progressRef} /></i></div>
        </div>
        <div ref={resultRef} className="os-create-screen__result os-moving"><TeapotVisual /></div>
      </div>
    </div>
  );
}

function SurfacePanel({ surface }: { surface: Surface }) {
  if (surface === "chat") return <ChatPanel />;
  if (surface === "create") return <CreatePanel />;
  if (surface === "memory") return <MemoryPanel />;
  return <CapsPanel />;
}

function Boot({ forcedProgress }: { forcedProgress?: number }) {
  const wordRef = useRef<HTMLDivElement>(null);
  const copyRef = useRef<HTMLDivElement>(null);
  const coreRef = useRef<HTMLDivElement>(null);
  const windowRef = useRef<HTMLDivElement>(null);
  const nodeRefs = useRef<(HTMLDivElement | null)[]>([]);
  const nodes = [
    { name: "AGENTS", code: "01", x: -330, y: -155, angle: -10 },
    { name: "MODÈLES", code: "02", x: 300, y: -130, angle: 11 },
    { name: "OUTILS", code: "03", x: -20, y: 215, angle: -5 },
  ];
  const move = (p: number, simplified: boolean) => {
    const gather = simplified ? 0 : smooth(segment(p, 0.31, 0.69));
    setStyle(wordRef.current, { y: simplified ? 0 : lerp(0, -160, smooth(segment(p, 0.1, 0.44))), opacity: simplified ? 1 : 1 - segment(p, 0.2, 0.53) });
    setStyle(copyRef.current, { y: simplified ? 0 : lerp(0, 28, segment(p, 0.43, 0.67)), opacity: simplified ? 1 : 1 - segment(p, 0.44, 0.68) });
    setStyle(coreRef.current, { scale: simplified ? 1 : lerp(0.88, 1.33, smooth(segment(p, 0.13, 0.73))), opacity: simplified ? 1 : 1 - segment(p, 0.77, 0.97) });
    nodes.forEach((node, i) => setStyle(nodeRefs.current[i], {
      x: lerp(node.x, 0, gather),
      y: lerp(node.y, 0, gather),
      rotate: lerp(node.angle, 0, gather),
      scale: lerp(1, 0.55, gather),
      opacity: simplified ? 1 : segment(p, 0.15, 0.3) * (1 - segment(p, 0.68, 0.82)),
    }));
    const reveal = simplified ? 0 : smooth(segment(p, 0.78, 0.97));
    setStyle(windowRef.current, { y: lerp(100, 0, reveal), scale: lerp(0.58, 1, reveal), rotateX: lerp(16, 0, reveal), opacity: reveal });
  };
  return (
    <section id="top" className="os-chapter os-chapter--boot" data-chapter="01">
      <PinnedScene label="Démarrage d'Akasha OS" height={300} forcedProgress={forcedProgress} onProgress={move}>
        <div className="os-boot">
          <div className="os-boot__status"><span>AKASHA OS / PREVIEW 0.19.0</span><span>LOCAL HOST APP</span></div>
          <div ref={wordRef} className="os-boot__word os-moving" aria-hidden="true">AKASHA</div>
          <div className="os-boot__scene" aria-hidden="true">
            <div className="os-boot__orbital os-boot__orbital--one" />
            <div className="os-boot__orbital os-boot__orbital--two" />
            {nodes.map((node, i) => <div key={node.name} ref={(el) => { nodeRefs.current[i] = el; }} className="os-boot__node os-moving"><small>{node.code} / SERVICE</small><strong>{node.name}</strong></div>)}
            <div ref={coreRef} className="os-boot__core os-moving"><span className="os-boot__core-back" /><img src={`${assetBase}akasha-mark.svg`} alt="" /><span className="os-boot__core-ring" /></div>
            <div ref={windowRef} className="os-boot__window os-moving"><OsWindow surface="chat" className="os-window--boot"><ChatPanel /></OsWindow></div>
          </div>
          <div ref={copyRef} className="os-boot__copy os-moving">
            <ChapterTag no="01 / 06" label="DÉMARRAGE" />
            <h1>Agents, modèles et outils comme services système.</h1>
            <p>La Preview est une application hôte locale pour Windows, Linux et Mac (Apple Silicon).</p>
          </div>
          <div className="os-boot__scroll">DÉFILER POUR ENTRER <ArrowDown size={16} /></div>
        </div>
      </PinnedScene>
    </section>
  );
}

function Desktop({ forcedProgress }: { forcedProgress?: number }) {
  const [current, setCurrent] = useState(0);
  const [mobileCurrent, setMobileCurrent] = useState(0);
  const frameRef = useRef<HTMLDivElement>(null);
  const panelRefs = useRef<(HTMLDivElement | null)[]>([]);
  const copyRefs = useRef<(HTMLDivElement | null)[]>([]);
  const ranges = [
    { a: 0, b: 0.21, c: 0.26 },
    { a: 0.27, b: 0.46, c: 0.51 },
    { a: 0.52, b: 0.71, c: 0.76 },
    { a: 0.77, b: 1, c: 1 },
  ];
  const move = (p: number, simplified: boolean) => {
    const enter = simplified ? 1 : smooth(segment(p, 0.02, 0.15));
    setStyle(frameRef.current, { y: lerp(45, 0, enter), scale: lerp(0.87, 1, enter), opacity: enter });
    if (simplified) return;
    const index = p < 0.26 ? 0 : p < 0.51 ? 1 : p < 0.76 ? 2 : 3;
    setCurrent((previous) => previous === index ? previous : index);
    ranges.forEach((range, i) => {
      const entry = i === 0 ? 1 : smooth(segment(p, range.a, range.a + 0.05));
      const exit = i === 3 ? 1 : 1 - smooth(segment(p, range.b, range.c));
      const visible = entry * exit;
      setStyle(panelRefs.current[i], { y: lerp(22, 0, entry), opacity: visible });
      setStyle(copyRefs.current[i], { y: lerp(16, 0, entry), opacity: visible });
    });
  };
  return (
    <section id="desktop" className="os-chapter os-chapter--desktop" data-chapter="02">
      <PinnedScene label="Les surfaces de la Preview" height={360} forcedProgress={forcedProgress} onProgress={move}>
        <div className="os-desktop">
          <div className="os-desktop__top"><ChapterTag no="02 / 06" label="LE BUREAU" /><h2>Un même rail.<br />Quatre surfaces.</h2></div>
          <div className="os-desktop__ghost" aria-hidden="true">{nav[current].name.toUpperCase()}</div>
          <div ref={frameRef} className="os-desktop__frame os-moving">
            <div className="os-desktop__panels">
              {nav.map(({ id }, i) => <div key={id} ref={(el) => { panelRefs.current[i] = el; }} className="os-desktop__panel os-moving"><OsWindow surface={id}><SurfacePanel surface={id} /></OsWindow></div>)}
            </div>
            <div className="os-desktop__mobile-panel"><OsWindow surface={nav[mobileCurrent].id}><SurfacePanel surface={nav[mobileCurrent].id} /></OsWindow></div>
          </div>
          <div className="os-desktop__foot">
            <div className="os-desktop__tabs" role="group" aria-label="Choisir une surface">
              {nav.map(({ id, name, Icon }, i) => <button key={id} type="button" aria-label={name} className={(current === i ? "is-active " : "") + (mobileCurrent === i ? "is-mobile-active" : "")} onClick={() => setMobileCurrent(i)}><Icon size={17} /><span>{name}</span></button>)}
            </div>
            <div className="os-desktop__copies">{nav.map(({ id, description }, i) => <p key={id} ref={(el) => { copyRefs.current[i] = el; }} className="os-moving">{description}</p>)}</div>
            <p className="os-desktop__mobile-copy">{nav[mobileCurrent].description}</p>
          </div>
        </div>
      </PinnedScene>
    </section>
  );
}

function Chat({ forcedProgress }: { forcedProgress?: number }) {
  const frameRef = useRef<HTMLDivElement>(null);
  const promptRef = useRef<HTMLDivElement>(null);
  const replyRef = useRef<HTMLDivElement>(null);
  const offlineRef = useRef<HTMLDivElement>(null);
  const move = (p: number, simplified: boolean) => {
    const enter = simplified ? 1 : smooth(segment(p, 0.03, 0.22));
    const prompt = simplified ? 1 : smooth(segment(p, 0.27, 0.42));
    const reply = simplified ? 1 : smooth(segment(p, 0.55, 0.7));
    const offline = simplified ? 1 : smooth(segment(p, 0.76, 0.89));
    setStyle(frameRef.current, { y: lerp(72, 0, enter), scale: lerp(0.8, 1, enter), rotateX: lerp(10, 0, enter), opacity: enter });
    setStyle(promptRef.current, { x: lerp(34, 0, prompt), opacity: prompt });
    setStyle(replyRef.current, { x: lerp(-34, 0, reply), opacity: reply });
    setStyle(offlineRef.current, { y: lerp(30, 0, offline), opacity: offline });
  };
  return (
    <section id="chat" className="os-chapter os-chapter--chat" data-chapter="03">
      <PinnedScene label="Chat local" height={270} forcedProgress={forcedProgress} onProgress={move}>
        <div className="os-chat-chapter">
          <div className="os-chat-chapter__vertical" aria-hidden="true">LOCAL / CHAT</div>
          <div className="os-chat-chapter__heading"><ChapterTag no="03 / 06" label="CHAT" /><h2>La conversation reste locale.</h2></div>
          <div ref={frameRef} className="os-chat-chapter__frame os-moving"><OsWindow surface="chat"><ChatPanel promptRef={promptRef} replyRef={replyRef} /></OsWindow></div>
          <div ref={offlineRef} className="os-chat-chapter__offline os-moving"><WifiOff size={19} /><span>HORS LIGNE PAR DÉFAUT</span><i /><p>Les outils web s'activent seulement si vous le choisissez.</p></div>
        </div>
      </PinnedScene>
    </section>
  );
}

function Create({ forcedProgress }: { forcedProgress?: number }) {
  const frameRef = useRef<HTMLDivElement>(null);
  const recipeRef = useRef<HTMLDivElement>(null);
  const jobRef = useRef<HTMLDivElement>(null);
  const resultRef = useRef<HTMLDivElement>(null);
  const progressRef = useRef<HTMLDivElement>(null);
  const titleRef = useRef<HTMLDivElement>(null);
  const move = (p: number, simplified: boolean) => {
    const enter = simplified ? 1 : smooth(segment(p, 0.03, 0.2));
    const recipe = simplified ? 1 : smooth(segment(p, 0.27, 0.43));
    const job = simplified ? 1 : smooth(segment(p, 0.5, 0.64));
    const progress = simplified ? 1 : smooth(segment(p, 0.64, 0.74));
    const result = simplified ? 1 : smooth(segment(p, 0.8, 0.94));
    setStyle(frameRef.current, { y: lerp(86, 0, enter), scale: lerp(0.78, 1, enter), rotateX: lerp(14, 0, enter), opacity: enter });
    setStyle(titleRef.current, { y: lerp(32, 0, enter), opacity: enter });
    setStyle(recipeRef.current, { y: lerp(20, 0, recipe), opacity: recipe });
    setStyle(jobRef.current, { y: lerp(20, 0, job), opacity: job });
    if (progressRef.current) progressRef.current.style.width = String(progress * 100) + "%";
    setStyle(resultRef.current, { scale: lerp(0.83, 1, result), opacity: result });
  };
  return (
    <section id="create" className="os-chapter os-chapter--create" data-chapter="04">
      <PinnedScene label="Create et les packs du catalogue" height={330} forcedProgress={forcedProgress} onProgress={move}>
        <div className="os-create-chapter">
          <div className="os-create-chapter__word" aria-hidden="true">CREATE</div>
          <div ref={titleRef} className="os-create-chapter__heading os-moving"><ChapterTag no="04 / 06" label="CREATE" /><h2>Un pack. Sa recette.<br />Votre génération.</h2><p>Les packs image et vidéo définissent les réglages. Un téléchargement incomplet bloque la génération. La progression reste sur le panneau Create.</p></div>
          <div ref={frameRef} className="os-create-chapter__frame os-moving"><OsWindow surface="create"><CreatePanel recipeRef={recipeRef} jobRef={jobRef} resultRef={resultRef} progressRef={progressRef} /></OsWindow></div>
          <span className="os-create-chapter__index">01 / PACK <i /> 02 / JOB <i /> 03 / RÉSULTAT</span>
        </div>
      </PinnedScene>
    </section>
  );
}

function Caps({ forcedProgress }: { forcedProgress?: number }) {
  const agentRef = useRef<HTMLDivElement>(null);
  const capRef = useRef<HTMLDivElement>(null);
  const blockRef = useRef<HTMLDivElement>(null);
  const auditRef = useRef<HTMLDivElement>(null);
  const pathRef = useRef<SVGPathElement>(null);
  const logRef = useRef<HTMLDivElement>(null);
  const move = (p: number, simplified: boolean) => {
    const agent = simplified ? 1 : smooth(segment(p, 0.08, 0.24));
    const cap = simplified ? 1 : smooth(segment(p, 0.31, 0.5));
    const block = simplified ? 1 : smooth(segment(p, 0.56, 0.72));
    const audit = simplified ? 1 : smooth(segment(p, 0.77, 0.91));
    setStyle(agentRef.current, { x: lerp(-80, 0, agent), opacity: agent });
    setStyle(capRef.current, { scale: lerp(0.64, 1, cap), opacity: cap });
    setStyle(blockRef.current, { x: lerp(80, 0, block), opacity: block });
    setStyle(auditRef.current, { y: lerp(38, 0, audit), opacity: audit });
    setStyle(logRef.current, { y: lerp(46, 0, audit), opacity: audit });
    if (pathRef.current) pathRef.current.style.strokeDashoffset = String(780 * (1 - cap));
  };
  return (
    <section id="caps" className="os-chapter os-chapter--caps" data-chapter="05">
      <PinnedScene label="Capacités révocables et audit" height={270} forcedProgress={forcedProgress} onProgress={move}>
        <div className="os-caps-chapter">
          <div className="os-caps-chapter__heading"><ChapterTag no="05 / 06" label="CAPS" /><h2>L'autonomie a des limites visibles.</h2><p>Sortie réseau refusée par défaut. Jetons révocables. Piste d'audit.</p></div>
          <div className="os-network" aria-hidden="true">
            <svg viewBox="0 0 1000 320" preserveAspectRatio="none"><path d="M100 160 C240 160 260 160 450 160 C600 160 660 160 880 160" /><path ref={pathRef} className="os-network__flow" d="M100 160 C240 160 260 160 500 160" /></svg>
            <div ref={agentRef} className="os-network__node os-network__node--agent os-moving"><span>01</span><Brain size={31} /><b>AGENT</b></div>
            <div ref={capRef} className="os-network__node os-network__node--cap os-moving"><span>02</span><Shield size={39} /><b>CAP TOKEN</b><small>RÉVOCABLE</small></div>
            <div ref={blockRef} className="os-network__node os-network__node--network os-moving"><span>03</span><WifiOff size={32} /><b>RÉSEAU</b><small>REFUSÉ PAR DÉFAUT</small></div>
            <div ref={auditRef} className="os-network__audit os-moving"><FileText size={20} /><span>AUDIT / ACTION CONSIGNÉE</span></div>
          </div>
          <div ref={logRef} className="os-caps-chapter__log os-moving"><span>CAPS / EXEMPLE</span><strong>Contrôler, révoquer, vérifier.</strong><p>Les agents restent bornés par leurs capacités.</p></div>
        </div>
      </PinnedScene>
    </section>
  );
}

function Install() {
  return (
    <section id="install" className="os-install" data-chapter="06">
      <div className="os-install__head"><ChapterTag no="06 / 06" label="PREMIER PARCOURS" /><h2>La Preview est prête<br />à être installée.</h2><p>Une application hôte locale. NVIDIA recommandé ; un parcours CPU est inclus dans le même paquet.</p></div>
      <div className="os-install__word" aria-hidden="true">PREVIEW</div>
      <div className="os-install__bottom">
        <ol><li><span>01</span>Installer la Preview</li><li><span>02</span>Faire un chat hors ligne</li><li><span>03</span>Écrire une note et envoyer un retour</li></ol>
        <a href={urls.install} target="_blank" rel="noreferrer"><span>OBTENIR LA PREVIEW</span><Download size={23} /></a>
      </div>
      <div className="os-install__platforms"><Monitor size={18} /> WINDOWS <i /> LINUX <i /> MAC APPLE SILICON <a href={urls.firstRun} target="_blank" rel="noreferrer">Premier lancement <ArrowUpRight size={15} /></a></div>
    </section>
  );
}

function Footer() {
  return <footer className="os-footer"><Brand /><div><a href={urls.docs} target="_blank" rel="noreferrer">Documentation</a><a href={urls.news} target="_blank" rel="noreferrer">Nouveautés</a><a href={urls.community} target="_blank" rel="noreferrer">Communauté</a><a href={urls.about} target="_blank" rel="noreferrer">À propos</a><a href={urls.github} target="_blank" rel="noreferrer">GitHub</a></div><small>Akasha OS / Preview 0.19.0</small></footer>;
}

export default function App() {
  const params = new URLSearchParams(window.location.search);
  const scene = params.get("scene") as Scene | null;
  const raw = Number(params.get("progress"));
  const forced = params.has("progress") && Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : undefined;
  const show = (id: Scene) => !scene || scene === id;
  const frame = (id: Scene) => scene === id ? forced : undefined;
  return (
    <div className="os-site">
      <a className="os-skip" href="#content">Aller au contenu</a>
      <Header />
      <main id="content">
        {show("boot") && <Boot forcedProgress={frame("boot")} />}
        {show("desktop") && <Desktop forcedProgress={frame("desktop")} />}
        {show("chat") && <Chat forcedProgress={frame("chat")} />}
        {show("create") && <Create forcedProgress={frame("create")} />}
        {show("caps") && <Caps forcedProgress={frame("caps")} />}
        {show("install") && <Install />}
      </main>
      {!scene && <Footer />}
    </div>
  );
}

