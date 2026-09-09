import { useEffect, useRef, useState, useCallback } from "react";
import { motion, MotionConfig, useReducedMotion } from "motion/react";
import { toast } from "sonner";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Cube, GearSix, Keyboard, Plugs, Sliders, Sparkle, Terminal, UserCircleGear } from "@phosphor-icons/react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { TitleBar } from "@/components/TitleBar";
import { SettingsViewport } from "./SettingsViewport";
import { ProjectsTab } from "./ProjectsTab";
import { ProfileEditor } from "./ProfileEditor";
import { ProvidersTab } from "./tabs/ProvidersTab";
import { MODE_COPY, RefiningTab } from "./tabs/RefiningTab";
import { ShortcutTab } from "./tabs/ShortcutTab";
import { AppearanceTab } from "./tabs/AppearanceTab";
import { AboutTab } from "./tabs/AboutTab";
import { DevTab } from "./tabs/DevTab";
import {
  DEFAULT_SETTINGS,
  ipc,
  type EmberSettings,
  type ProviderHealth,
  type ProviderKind,
  type RefineMode,
  type Length,
  type OrbSkin,
  type OrbSize,
  type NoticeSpeed,
  type Theme,
  type ThinkingLevel,
  type ModelCatalog,
  type HotkeySlot,
} from "@/lib/ipc";

/** Aplica o tema no <html> via data-theme. O CSS (globals.css) faz o resto: dark e o default
 *  (sem atributo ou "dark"); "cream" liga o bloco :root[data-theme="cream"]. */
function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

/** Says which mode was chosen, once the walk through the list settles. */
function useSettledToast(delayMs: number) {
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);
  return (message: string) => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => toast.success(message), delayMs);
  };
}

/**
 * The settings shell: state, IPC callbacks, title bar and the tab strip. Each tab body lives
 * in `./tabs/*` and lays itself out from the container the shell gives it (see
 * `SettingsViewport`). The window never scrolls as a page: `main` is exactly the viewport and
 * clips, every tab fits it, and only editors, lists and dialog bodies scroll inside.
 */
export function Settings({ initialTab = "providers" }: { initialTab?: string } = {}) {
  // The window is native and lives for the whole session, shown and hidden around this one
  // component. `phase` is what the person sees: "open" is the content at full opacity, "hidden"
  // is the content faded out. A close is a fade and then a native hide (Rust emits
  // `settings-closing`, waits for the fade and hides whatever happened here), and the content
  // stays faded out while hidden, so the next show never paints the previous content for a frame
  // before the entrance runs. The old approach remounted everything on each open (`openKey`),
  // which is exactly what produced that frame and the skeleton flash behind it.
  const still = useReducedMotion();
  const settledToast = useSettledToast(400);
  const announceMode = (mode: RefineMode) => settledToast(`Refine mode: ${MODE_COPY[mode].title}.`);
  const [phase, setPhase] = useState<"hidden" | "open">("hidden");
  const open = phase === "open";
  const [tab, setTab] = useState(initialTab);
  const [s, setS] = useState<EmberSettings>(DEFAULT_SETTINGS);
  const [hotkey, setHotkey] = useState(DEFAULT_SETTINGS.hotkey);
  // Saude dos providers, ao nivel do Settings, para refazer quando uma chave muda (Bug C) e
  // passar ja resolvida ao aviso (que deixa de ter useEffect proprio).
  const [health, setHealth] = useState<ProviderHealth | null>(null);
  // Listagens de modelos por provider, descobertas em runtime. Nao bloqueiam nada: ate
  // chegarem, os selects mostram a lista embutida e dizem que e essa.
  const [catalogs, setCatalogs] = useState<Partial<Record<ProviderKind, ModelCatalog>>>({});
  const [healthDismissed, setHealthDismissed] = useState(false);
  // Ate o getSettings assincrono voltar, `s` sao os defaults. Mostrar os tabs ja com defaults
  // pisca um estado falso (ex.: "sem chave" antes da chave real aterrar). Segura o conteudo ate
  // hidratar. Fica true tambem no catch (fora do Tauri: renderiza com defaults, sem ficar preso).
  const [hydrated, setHydrated] = useState(false);
  const refreshHealth = () =>
    ipc.getProviderHealth().then(setHealth).catch(() => {
      /* cofre ilegivel / fora do Tauri: o banner de key-store trata o caso grave */
    });
  /** Rebusca as listagens. Best-effort: uma falha deixa o select como estava, sem toast, porque
   *  nao ha nada que o utilizador possa fazer e a lista embutida continua a servir. */
  const refreshCatalogs = () => {
    (["gemini", "openai"] as ProviderKind[]).forEach((kind) => {
      ipc
        .listModels(kind)
        .then((c) => setCatalogs((prev) => ({ ...prev, [kind]: c })))
        .catch(() => {});
    });
  };
  /** Grava um dos tres atalhos. Em caso de recusa mostra a mensagem do SO (que diz se a
   *  combinacao e invalida ou se ja esta ocupada por outra app), em vez de um erro generico
   *  que deixava o utilizador sem saber o que tentar a seguir. O Rust ja restaurou o conjunto
   *  anterior, por isso a app nunca fica sem atalho por causa de uma tentativa falhada. */
  const commitHotkey = async (
    which: HotkeySlot,
    accel: string,
  ): Promise<string | null> => {
    try {
      await ipc.setHotkey(which, accel);
      const res = await ipc.getSettings();
      setS(res);
      setHotkey(res.hotkey);
      toast.success(accel ? `Shortcut set to ${accel}.` : "Shortcut cleared.");
      return null;
    } catch (e) {
      // A mensagem volta para o alerta inline da HotkeyCapture (um canal de erro so); o toast
      // fica reservado ao sucesso. Um erro passageiro num canto nao ensina o que tentar a seguir.
      return `Couldn't apply that shortcut. ${String(e)}`;
    }
  };
  /** Põe este provider à frente na ordem de tentativa. Só muda a ordem: chaves, sessão e modelos
   *  ficam onde estavam, e é por isso que voltar atrás é um clique no outro cartão. */
  const makePrimary = (kind: ProviderKind) => {
    ipc
      .setPrimaryProvider(kind)
      .then((next) => {
        setS(next);
        refreshHealth();
        toast.success(
          kind === "gemini"
            ? "Gemini is now tried first."
            : "The fallback service is now tried first."
        );
      })
      .catch(() => toast.error("Couldn't change which one goes first."));
  };

  useEffect(() => {
    // Close (X / Alt+F4) is native: Rust prevents the destroy, emits `settings-closing`, waits
    // for the fade below and hides the window itself; the app stays in the tray. No close
    // handler in JS on purpose: the webview's own was fragile and left the window stuck black
    // when it failed.
    //
    // Reopen: the window already exists (only hidden) and Rust emits `settings-opened`. The data
    // is fetched again, because the window was created at startup and what it showed may be
    // stale (a project switched from the picker, a key saved elsewhere), and the content comes
    // back from the faded state. `hydrated` stays true, so no skeleton flashes in between.
    const unlistenOpen = listen("settings-opened", () => {
      setPhase("open");
      loadSettings();
    });
    const unlistenClose = listen("settings-closing", () => setPhase("hidden"));
    // Safety nets for a window shown without the event: the on-demand path creates the window
    // and shows it before this listener exists, and a lost event would otherwise leave a blank
    // window. Visible at mount, or focused later, means open. Only an explicit `false` from the
    // window keeps the content faded: outside Tauri (the browser fixtures) there is no answer,
    // and the content must simply be there.
    let unlistenFocus: (() => void) | undefined;
    try {
      const win = getCurrentWindow();
      win
        .isVisible()
        .then((visible) => { if (visible !== false) setPhase("open"); })
        .catch(() => setPhase("open"));
      win
        .onFocusChanged(({ payload: focused }) => { if (focused) setPhase("open"); })
        .then((stop) => { unlistenFocus = stop; })
        .catch(() => {});
    } catch {
      setPhase("open");
    }

    return () => {
      unlistenOpen.then((f) => f());
      unlistenClose.then((f) => f());
      unlistenFocus?.();
    };
  }, []);

  /** Traz o estado do Rust para o ecra. Corre na montagem E a cada reabertura da janela: como
   *  a janela e criada no ARRANQUE e depois so escondida/mostrada, sem isto ficava a mostrar o
   *  que era verdade quando a app abriu (o projeto ativo mudado pelo picker, um atalho limpo na
   *  sanitizacao da config, uma chave gravada noutro sitio). */
  const loadSettings = useCallback(() => {
    ipc
      .getSettings()
      .then((res) => {
        setS(res);
        setHotkey(res.hotkey);
        refreshCatalogs();
        applyTheme(res.theme);
      })
      .catch(() => {
        /* outside Tauri: use defaults */
      })
      .finally(() => setHydrated(true));
    refreshHealth();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  /** Os tres numa chamada so, porque viajam juntos para a janela do overlay. Otimista com
   *  reversao, como todos os outros: a mudanca vale a pena ver na pre-visualizacao antes de o
   *  disco confirmar. */
  const setOverlayStyle = (orbSkin: OrbSkin, orbSize: OrbSize, noticeSpeed: NoticeSpeed) => {
    const prev = { orbSkin: s.orbSkin, orbSize: s.orbSize, noticeSpeed: s.noticeSpeed };
    setS({ ...s, orbSkin, orbSize, noticeSpeed });
    ipc.setOverlayStyle(orbSkin, orbSize, noticeSpeed).catch(() => {
      setS((cur) => ({ ...cur, ...prev }));
      toast.error("Couldn't change the overlay style.");
    });
  };

  /** O tamanho nao tem toast: e uma preferencia silenciosa, ao contrario do modo, que muda o
   *  que o atalho principal faz e por isso se anuncia. */
  const setLength = (length: Length) => {
    const prev = s.length;
    setS({ ...s, length });
    ipc.setLength(length).catch(() => {
      setS((cur) => ({ ...cur, length: prev }));
      toast.error("Couldn't update the length.");
    });
  };

  // Turning Developer tools off while standing on its tab would leave Radix pointing at a
  // trigger that no longer exists, and the panel would render empty with no way back. Step to
  // About, which is where the switch that just moved lives.
  useEffect(() => {
    if (!s.debugMode && tab === "dev") setTab("about");
  }, [s.debugMode, tab]);

  const setMode = (mode: RefineMode) => {
    const prev = s.mode;
    setS({ ...s, mode });
    ipc
      .setMode(mode)
      // Only the toast waits. Arrow keys walk the comparison and fire one write per keypress;
      // the write is a cheap local one where the last caller wins, but three toasts stacking up
      // for one deliberate move through the list is noise.
      .then(() => announceMode(mode))
      .catch(() => {
        setS((cur) => ({ ...cur, mode: prev })); // reverte o otimismo se o backend falhou
        toast.error("Couldn't update the mode.");
      });
  };

  const setTheme = (theme: Theme) => {
    const prev = s.theme;
    setS({ ...s, theme });
    applyTheme(theme); // aplica ja (otimista); o data-theme troca as cores na hora
    ipc.setTheme(theme).catch(() => {
      setS((cur) => ({ ...cur, theme: prev }));
      applyTheme(prev);
      toast.error("Couldn't change the theme.");
    });
  };

  const setThinking = (enabled: boolean, level: ThinkingLevel) => {
    const prev = { enabled: s.thinkingEnabled, level: s.thinkingLevel };
    setS({ ...s, thinkingEnabled: enabled, thinkingLevel: level });
    ipc.setThinking(enabled, level).catch(() => {
      setS((cur) => ({ ...cur, thinkingEnabled: prev.enabled, thinkingLevel: prev.level }));
      toast.error("Couldn't update extended thinking.");
    });
  };

  return (
    <MotionConfig reducedMotion="user">
      {/* No remount and no AnimatePresence: the same node fades between the two phases. A key
          swap per open painted the old content, then nothing, then the entrance; an exit-then-
          enter did the "shows, vanishes, shows" of the reopen. Here the content is already faded
          out when the window shows, and the entrance is the only thing that moves. */}
      <motion.main
        // `h-screen overflow-hidden`: the page is the viewport and nothing scrolls it. The
        // animation only touches opacity and transform, so it never changes the layout the
        // tabs measure themselves against.
        className="flex h-screen flex-col overflow-hidden bg-panel text-fg"
        // No `initial`: the first render paints the phase as it is, so a pre-warmed window sits
        // faded out until its first open and a window shown on the spot animates in from there.
        // 240ms in, 140ms out. The out is mirrored by SETTINGS_CLOSE_MS in src-tauri/src/lib.rs,
        // which hides the window after that time; change one, change the other. The scale
        // starts at 0.985 so the text does not visibly drag.
        initial={false}
        animate={open ? { opacity: 1, scale: 1 } : { opacity: 0, scale: 0.985 }}
        transition={
          still
            ? { duration: 0 }
            : open
              ? { duration: 0.24, ease: [0.22, 1, 0.36, 1] }
              : { duration: 0.14, ease: [0.4, 0, 1, 1] }
        }
        style={{ transformOrigin: "center" }}
        data-phase={phase}
      >
        <TitleBar />
        <motion.div
          className="flex min-h-0 flex-1 flex-col"
          // Follows the outer one closely (short delay) instead of adding half a second on top:
          // the two chained used to take about 800ms to settle. On the way out it simply goes
          // with the outer fade.
          initial={false}
          animate={open ? { opacity: 1, y: 0 } : { opacity: 0, y: 6 }}
          transition={
            still
              ? { duration: 0 }
              : open
                ? { duration: 0.3, ease: [0.22, 1, 0.36, 1], delay: 0.05 }
                : { duration: 0.14, ease: [0.4, 0, 1, 1] }
          }
        >
          <SettingsViewport>
            {!hydrated ? (
              // Esqueleto enquanto o getSettings nao voltou: evita piscar um estado falso (ex.:
              // "sem chave" antes da chave real aterrar). So opacidade anima (compositor-only).
              <div className="flex flex-col gap-4" aria-busy="true" aria-live="polite">
                <span className="sr-only">Loading settings</span>
                <div className="h-10 w-full animate-pulse rounded-lg bg-surface-1" />
                <div className="h-32 w-full animate-pulse rounded-lg bg-surface-1" />
                <div className="h-32 w-full animate-pulse rounded-lg bg-surface-1" />
              </div>
            ) : (
              <Tabs value={tab} onValueChange={setTab} className="flex min-h-0 flex-1 flex-col">
                <TabsList className="shrink-0">
                  <TabsTrigger value="providers">
                    <Plugs size={16} /> Providers
                  </TabsTrigger>
                  <TabsTrigger value="refining">
                    <Sliders size={16} /> Refining
                  </TabsTrigger>
                  <TabsTrigger value="hotkey">
                    <Keyboard size={16} /> Shortcut
                  </TabsTrigger>
                  <TabsTrigger value="projects">
                    <Cube size={16} /> Projects
                  </TabsTrigger>
                  <TabsTrigger value="profile">
                    <UserCircleGear size={16} /> Profile
                  </TabsTrigger>
                  <TabsTrigger value="appearance">
                    <GearSix size={16} /> Appearance
                  </TabsTrigger>
                  <TabsTrigger value="about">
                    <Sparkle size={16} /> About
                  </TabsTrigger>
                  {s.debugMode && (
                    <TabsTrigger value="dev">
                      <Terminal size={16} /> Dev
                    </TabsTrigger>
                  )}
                </TabsList>

                <TabsContent value="providers">
                  <ProvidersTab
                    s={s}
                    setS={setS}
                    health={health}
                    healthDismissed={healthDismissed}
                    onDismissHealth={() => setHealthDismissed(true)}
                    catalogs={catalogs}
                    refreshHealth={refreshHealth}
                    refreshCatalogs={refreshCatalogs}
                    makePrimary={makePrimary}
                  />
                </TabsContent>

                <TabsContent value="refining">
                  <RefiningTab s={s} setS={setS} setMode={setMode} setLength={setLength} setThinking={setThinking} />
                </TabsContent>

                <TabsContent value="hotkey">
                  <ShortcutTab s={s} setS={setS} hotkey={hotkey} commitHotkey={commitHotkey} />
                </TabsContent>

                <TabsContent value="projects">
                  <ProjectsTab s={s} setS={setS} />
                </TabsContent>

                <TabsContent value="profile">
                  <ProfileEditor settings={s} onSaved={updated => setS(current => ({ ...current,
                    profileText: updated.profileText, profileSource: updated.profileSource,
                    profileReview: updated.profileReview, profileArchive: updated.profileArchive,
                    profilePath: updated.profilePath, profileSources: updated.profileSources,
                    legacyAutoProfileDisabled: updated.legacyAutoProfileDisabled,
                  }))} />
                </TabsContent>

                <TabsContent value="appearance">
                  <AppearanceTab s={s} setTheme={setTheme} setOverlayStyle={setOverlayStyle} />
                </TabsContent>

                <TabsContent value="about">
                  <AboutTab s={s} setS={setS} />
                </TabsContent>

                {s.debugMode && (
                  <TabsContent value="dev">
                    <DevTab s={s} />
                  </TabsContent>
                )}
              </Tabs>
            )}
          </SettingsViewport>
        </motion.div>
      </motion.main>
    </MotionConfig>
  );
}
