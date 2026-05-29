<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import "@fontsource/instrument-serif/400.css";
  import "@fontsource/instrument-serif/400-italic.css";
  import "@fontsource/archivo/400.css";
  import "@fontsource/archivo/500.css";
  import "@fontsource/archivo/600.css";
  import "@fontsource/archivo/700.css";

  type SpanInfo = { id: number; category: string; source: string; text: string; replacement: string };
  type Segment = { text: string; span: number | null };
  type AnalyzeResult = {
    text: string;
    segments: Segment[];
    spans: SpanInfo[];
    counts: Record<string, number>;
    warnings: string[];
  };

  const CATEGORIES = [
    { key: "person", label: "Person", color: "#e11d48" },
    { key: "personnummer", label: "Personnummer", color: "#be123c" },
    { key: "plats", label: "Plats", color: "#2563eb" },
    { key: "organisation", label: "Organisation", color: "#7c3aed" },
    { key: "telefon", label: "Telefon", color: "#0891b2" },
    { key: "epost", label: "E-post", color: "#059669" },
    { key: "ip_adress", label: "IP-adress", color: "#4f46e5" },
    { key: "tid", label: "Tid", color: "#d97706" },
    { key: "handelse", label: "Händelse", color: "#0d9488" },
    { key: "diagnos", label: "Diagnos", color: "#b45309" },
    { key: "egen", label: "Egen ordlista", color: "#db2777" },
    { key: "ovrigt", label: "Övrigt (AI)", color: "#64748b" },
  ];
  const ALL_KEYS = CATEGORIES.map((c) => c.key);
  const colorOf = (key: string) => CATEGORIES.find((c) => c.key === key)?.color ?? "#888";

  const IDENTITY = ["person", "personnummer", "telefon", "epost", "ip_adress", "plats", "organisation", "egen", "ovrigt"];
  const PROFILES = [
    { id: "allman", label: "Allmän", cats: IDENTITY },
    { id: "skola", label: "Skola / Elevhälsa", cats: [...IDENTITY, "diagnos", "tid"] },
    { id: "social", label: "Socialtjänst", cats: [...IDENTITY, "diagnos", "tid", "handelse"] },
    { id: "allt", label: "Allt", cats: ALL_KEYS },
  ];
  function profileMap(id: string): Record<string, boolean> {
    const set = new Set(PROFILES.find((p) => p.id === id)?.cats ?? ALL_KEYS);
    return Object.fromEntries(ALL_KEYS.map((k) => [k, set.has(k)]));
  }

  let inputText = $state("");
  let sourcePath = $state<string | null>(null);
  let sourceName = $state<string | null>(null);

  let selectedProfile = $state("skola");
  let enabled = $state<Record<string, boolean>>(profileMap("skola"));

  function applyProfile(id: string) {
    selectedProfile = id;
    const next = profileMap(id);
    for (const k of ALL_KEYS) enabled[k] = next[k];
  }
  let terms = $state<string[]>([]);
  let termInput = $state("");

  let analysis = $state<AnalyzeResult | null>(null);
  let rejected = $state<Set<number>>(new Set());
  let loading = $state(false);
  let useAi = $state(false);
  let status = $state("");
  let error = $state("");
  let toast = $state("");

  // Load saved dictionary terms.
  $effect(() => {
    const saved = localStorage.getItem("avident_terms");
    if (saved) terms = JSON.parse(saved);
  });

  function saveTerms() {
    localStorage.setItem("avident_terms", JSON.stringify(terms));
  }

  function showToast(msg: string) {
    toast = msg;
    setTimeout(() => (toast = ""), 2200);
  }

  function isActive(id: number): boolean {
    if (!analysis) return false;
    const span = analysis.spans[id];
    return enabled[span.category] && !rejected.has(id);
  }

  let activeCount = $derived(analysis ? analysis.spans.filter((s) => isActive(s.id)).length : 0);

  function countFor(key: string): number {
    if (!analysis) return 0;
    return analysis.spans.filter((s) => s.category === key).length;
  }

  async function analyze() {
    if (!sourcePath && inputText.trim() === "") return;
    loading = true;
    error = "";
    status = "";
    try {
      const result: AnalyzeResult = sourcePath
        ? await invoke("analyze_file", { path: sourcePath, enabled: ALL_KEYS, terms, useAi })
        : await invoke("analyze_text", { text: inputText, enabled: ALL_KEYS, terms, useAi });
      analysis = result;
      rejected = new Set();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function openFile() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Dokument", extensions: ["txt", "md", "docx"] }],
    });
    if (typeof selected === "string") {
      sourcePath = selected;
      sourceName = selected.split(/[\\/]/).pop() ?? selected;
      inputText = "";
      await analyze();
    }
  }

  function clearFile() {
    sourcePath = null;
    sourceName = null;
    analysis = null;
    status = "";
  }

  function toggleSpan(id: number) {
    if (!enabled[analysis!.spans[id].category]) return;
    const next = new Set(rejected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    rejected = next;
  }

  function addTerms() {
    const incoming = termInput
      .split(/[\n,;]+/)
      .map((t) => t.trim())
      .filter((t) => t.length > 0);
    if (incoming.length === 0) return;
    const set = new Set(terms);
    for (const t of incoming) set.add(t);
    terms = [...set].sort((a, b) => a.localeCompare(b, "sv"));
    saveTerms();
    termInput = "";
    if (analysis) analyze();
  }

  function removeTerm(t: string) {
    terms = terms.filter((x) => x !== t);
    saveTerms();
    if (analysis) analyze();
  }

  // Spans that should NOT be masked: disabled by category or individually turned off.
  function rejectedIds(): number[] {
    return analysis ? analysis.spans.filter((s) => !isActive(s.id)).map((s) => s.id) : [];
  }

  async function copyText() {
    if (!analysis) return;
    try {
      const text: string = await invoke("anonymized_text", { rejected: rejectedIds() });
      await navigator.clipboard.writeText(text);
      showToast("Kopierat till urklipp");
    } catch (e) {
      error = String(e);
    }
  }

  async function saveAs(ext: "txt" | "docx") {
    if (!analysis) return;
    const stem = sourceName ? sourceName.replace(/\.[^.]+$/, "") : "avidentifierad";
    const path = await save({
      defaultPath: `${stem}_avidentifierad.${ext}`,
      filters: [{ name: ext === "docx" ? "Word" : "Text", extensions: [ext] }],
    });
    if (!path) return;
    try {
      await invoke("export", { path, rejected: rejectedIds() });
      showToast("Filen sparades");
      status = path;
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="app">
  <header>
    <svg class="logo" viewBox="0 0 48 48" fill="none" aria-hidden="true">
      <rect x="11" y="6" width="22" height="32" rx="2" fill="#fff" stroke="#111214" stroke-width="2" />
      <rect x="16" y="13" width="12" height="3" fill="#111214" />
      <rect x="16" y="19" width="12" height="3" fill="#2440ff" />
      <rect x="16" y="25" width="7" height="3" fill="#c9ccd2" />
      <path d="M28 30l4-1 4 1v4.2c0 3.3-2.4 5.2-4 5.8-1.6-.6-4-2.5-4-5.8V30z" fill="#111214" />
      <path d="M30.4 34.4l1.4 1.4 2.4-2.6" stroke="#fff" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
    <div class="brand">
      <h1>Avidentifierare</h1>
      <p>Maskar känsliga uppgifter — tryggt nog att klistra in i en AI</p>
    </div>
    <div class="spacer"></div>
    <div class="lockbadge"><span class="dot"></span> Allt körs lokalt</div>
  </header>

  <div class="layout">
    <aside class="sidebar">
      <section>
        <h2>Text</h2>
        {#if sourceName}
          <div class="file-chip">
            <span title={sourcePath}>{sourceName}</span>
            <button class="link" onclick={clearFile}>rensa</button>
          </div>
        {:else}
          <textarea bind:value={inputText} placeholder="Klistra in text här…" rows="6"></textarea>
        {/if}
        <div class="row">
          <button class="btn primary grow" onclick={analyze} disabled={loading || (!sourcePath && inputText.trim() === "")}>
            {loading ? "Analyserar…" : "Analysera"}
          </button>
          <button class="btn" onclick={openFile} disabled={loading}>Öppna fil…</button>
        </div>
        <label class="ai-toggle">
          <input type="checkbox" bind:checked={useAi} />
          <span>Djupare granskning (AI) <em>långsammare (~20 s), fångar fler kontextuella ledtrådar</em></span>
        </label>
      </section>

      <section>
        <h2>Profil</h2>
        <select class="profile" value={selectedProfile} onchange={(e) => applyProfile(e.currentTarget.value)}>
          {#each PROFILES as p (p.id)}
            <option value={p.id}>{p.label}</option>
          {/each}
        </select>
      </section>

      <section>
        <h2>Kategorier</h2>
        <ul class="filters">
          {#each CATEGORIES as cat (cat.key)}
            <li>
              <label>
                <input type="checkbox" bind:checked={enabled[cat.key]} />
                <span class="dotc" style="background:{cat.color}"></span>
                {cat.label}
              </label>
              <span class="count">{countFor(cat.key)}</span>
            </li>
          {/each}
        </ul>
      </section>

      <section>
        <h2>Egen ordlista</h2>
        <textarea bind:value={termInput} placeholder="Lägg till ord att alltid maska. Ett per rad för flera." rows="3"></textarea>
        <button class="btn block" onclick={addTerms} disabled={termInput.trim() === ""}>Lägg till</button>
        {#if terms.length > 0}
          <ul class="terms">
            {#each terms as t (t)}
              <li><span>{t}</span><button class="x" onclick={() => removeTerm(t)} aria-label="Ta bort">×</button></li>
            {/each}
          </ul>
        {/if}
      </section>
    </aside>

    <main class="review">
      {#if error}
        <div class="banner error">{error}</div>
      {/if}
      {#if analysis?.warnings.length}
        {#each analysis.warnings as w}
          <div class="banner warn">{w}</div>
        {/each}
      {/if}

      {#if loading}
        <div class="state">
          <div class="spinner"></div>
          <p class="state-title">{useAi ? "Analyserar med AI…" : "Analyserar…"}</p>
          <p class="state-sub">{useAi ? "Den lokala AI-modellen granskar texten. Det kan ta ~20 sekunder." : "Modellen laddas första gången."}</p>
        </div>
      {:else if !analysis}
        <div class="state">
          <svg class="state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 5h16M4 10h16M4 15h10"/></svg>
          <p class="state-title">Klistra in text eller öppna en fil</p>
          <p class="state-sub">Klicka <strong>Analysera</strong>, granska de markerade träffarna och kopiera eller spara en avidentifierad version.</p>
        </div>
      {:else}
        <div class="review-head">
          <div class="meta"><strong>{activeCount}</strong> av {analysis.spans.length} träffar avidentifieras</div>
          <div class="actions">
            <button class="btn primary" onclick={copyText}>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="11" height="11" rx="1"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg>
              Kopiera text
            </button>
            <button class="btn" onclick={() => saveAs("txt")}>.txt</button>
            <button class="btn" onclick={() => saveAs("docx")}>Word</button>
          </div>
        </div>

        <div class="document">{#each analysis.segments as seg}{#if seg.span === null}{seg.text}{:else}{@const info = analysis.spans[seg.span]}{@const active = isActive(seg.span)}{@const off = !enabled[info.category]}<button
                class="hit"
                class:active
                class:rejected={!active && !off}
                class:disabled={off}
                style="--c:{colorOf(info.category)}"
                title={active ? `${info.text} → ${info.replacement}` : info.text}
                onclick={() => toggleSpan(seg.span!)}
              >{seg.text}</button>{/if}{/each}</div>

        <div class="reassure">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 3l8 4v5c0 5-3.4 7.7-8 9-4.6-1.3-8-4-8-9V7l8-4z"/><path d="M9 12l2 2 4-4"/></svg>
          Granska alltid träffarna innan du delar texten. Ingen automatik fångar 100 %.
        </div>
      {/if}
    </main>
  </div>
</div>

{#if toast}
  <div class="toast"><span class="accentbar"></span><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6"><path d="M5 13l4 4L19 7"/></svg>{toast}</div>
{/if}

<style>
  :global(:root) {
    --ink: #111214;
    --muted: #6a6e74;
    --faint: #a2a6ac;
    --bg: #ffffff;
    --line: #ececec;
    --line-2: #e2e2e2;
    --accent: #2440ff;
  }
  :global(body) {
    margin: 0;
    font-family: "Archivo", system-ui, sans-serif;
    color: var(--ink);
    background: var(--bg);
    -webkit-font-smoothing: antialiased;
  }
  .app { height: 100vh; display: flex; flex-direction: column; }

  header { display: flex; align-items: flex-end; gap: 15px; padding: 20px 30px 16px; border-bottom: 1px solid var(--line); }
  .logo { width: 36px; height: 36px; flex: none; margin-bottom: 3px; }
  .brand h1 { font-family: "Instrument Serif", serif; font-weight: 400; font-size: 34px; line-height: .9; margin: 0; }
  .brand p { margin: 5px 0 0; font-size: 13px; color: var(--muted); }
  .spacer { flex: 1; }
  .lockbadge { display: inline-flex; align-items: center; gap: 8px; font-size: 11.5px; letter-spacing: .05em; text-transform: uppercase; color: var(--muted); margin-bottom: 5px; }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: #16a34a; box-shadow: 0 0 0 3px rgba(22,163,74,.15); }

  .layout { flex: 1; display: grid; grid-template-columns: 300px 1fr; overflow: hidden; }
  .sidebar { padding: 22px 24px; overflow: auto; border-right: 1px solid var(--line); }
  section { margin-bottom: 26px; }
  h2 { font-size: 11px; letter-spacing: .15em; text-transform: uppercase; color: var(--faint); margin: 0 0 11px; font-weight: 600; }

  textarea, select.profile {
    width: 100%; box-sizing: border-box; font: inherit; font-size: 14px; color: var(--ink);
    border: 1px solid var(--line-2); border-radius: 3px; padding: 10px 12px; background: var(--bg);
  }
  textarea { resize: vertical; }
  select.profile { appearance: none; cursor: pointer;
    background-image: linear-gradient(45deg, transparent 50%, var(--ink) 50%), linear-gradient(135deg, var(--ink) 50%, transparent 50%);
    background-position: calc(100% - 18px) 18px, calc(100% - 13px) 18px; background-size: 5px 5px, 5px 5px; background-repeat: no-repeat; }

  .row { display: flex; gap: 8px; margin-top: 9px; }
  .btn { font: inherit; font-size: 13.5px; font-weight: 500; border: 1px solid var(--ink); background: var(--bg); color: var(--ink); border-radius: 3px; padding: 9px 14px; cursor: pointer; transition: .14s; display: inline-flex; align-items: center; justify-content: center; gap: 7px; }
  .btn:hover:not(:disabled) { background: var(--ink); color: #fff; }
  .btn:disabled { opacity: .4; cursor: default; }
  .btn svg { width: 15px; height: 15px; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover:not(:disabled) { background: var(--accent); filter: brightness(1.12); }
  .btn.grow { flex: 1; }
  .btn.block { width: 100%; margin-top: 8px; }
  .link { border: none; background: none; color: var(--accent); cursor: pointer; font: inherit; font-size: 13px; padding: 0 2px; }
  .x { border: none; background: none; color: var(--muted); cursor: pointer; font-size: 16px; line-height: 1; padding: 0 2px; }

  .ai-toggle { display: flex; align-items: flex-start; gap: 9px; margin-top: 12px; font-size: 13px; color: var(--muted); }
  .ai-toggle em { font-style: normal; color: var(--faint); display: block; margin-top: 2px; font-size: 12px; }

  .file-chip { display: flex; justify-content: space-between; align-items: center; gap: 8px; background: #f6f7ff; border: 1px solid #dfe3ff; border-radius: 3px; padding: 9px 11px; font-size: 13.5px; }
  .file-chip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .filters { list-style: none; margin: 0; padding: 0; }
  .filters li { display: flex; align-items: center; justify-content: space-between; padding: 6px 0; border-bottom: 1px solid var(--line); }
  .filters li:last-child { border-bottom: none; }
  .filters label { display: flex; align-items: center; gap: 10px; font-size: 13.5px; cursor: pointer; }
  .dotc { width: 9px; height: 9px; border-radius: 50%; flex: none; }
  .count { font-size: 12px; color: var(--faint); font-variant-numeric: tabular-nums; }
  input[type="checkbox"] { width: 15px; height: 15px; accent-color: var(--ink); }

  .terms { list-style: none; margin: 10px 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 6px; }
  .terms li { display: flex; align-items: center; gap: 4px; border: 1px solid var(--line-2); border-radius: 3px; padding: 3px 4px 3px 9px; font-size: 12.5px; }

  .review { padding: 24px 30px; display: flex; flex-direction: column; overflow: hidden; }
  .review-head { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; margin-bottom: 16px; }
  .review-head .meta { font-size: 13px; color: var(--muted); }
  .review-head .meta strong { color: var(--ink); font-weight: 700; }
  .actions { display: flex; gap: 8px; }

  .document {
    flex: 1; overflow: auto; white-space: pre-wrap; line-height: 2.1; font-size: 16.5px;
    max-width: 74ch; padding: 4px 2px;
  }
  .hit { border: none; background: none; font: inherit; line-height: inherit; cursor: pointer; padding: 0 1px 1px; border-bottom: 2px solid var(--c); transition: background .14s; color: inherit; }
  .hit:hover { background: color-mix(in srgb, var(--c) 13%, transparent); }
  .hit.rejected { border-bottom: 2px dotted var(--faint); text-decoration: line-through; color: var(--faint); }
  .hit.disabled { border-bottom: none; color: inherit; cursor: default; }

  .reassure { margin-top: 18px; padding-top: 15px; border-top: 1px solid var(--line); font-size: 12.5px; color: var(--muted); display: flex; align-items: center; gap: 9px; }
  .reassure svg { width: 15px; height: 15px; color: var(--accent); }

  .state { margin: auto; text-align: center; color: var(--muted); max-width: 380px; padding: 30px; }
  .state-icon { width: 40px; height: 40px; color: var(--line-2); margin-bottom: 14px; }
  .state-title { font-family: "Instrument Serif", serif; font-size: 24px; color: var(--ink); margin: 0 0 6px; }
  .state-sub { font-size: 14px; margin: 0; line-height: 1.6; }
  .spinner { width: 34px; height: 34px; border: 3px solid var(--line); border-top-color: var(--accent); border-radius: 50%; margin: 0 auto 16px; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .banner { border-radius: 3px; padding: 10px 13px; margin-bottom: 12px; font-size: 13.5px; }
  .banner.error { background: #fef2f2; color: #b91c1c; border: 1px solid #fecaca; }
  .banner.warn { background: #fffbeb; color: #92400e; border: 1px solid #fde68a; }

  .toast { position: fixed; bottom: 28px; left: 50%; transform: translateX(-50%); background: var(--ink); color: #fff; padding: 12px 18px 12px 20px; border-radius: 3px; font-size: 13.5px; font-weight: 500; display: flex; align-items: center; gap: 9px; box-shadow: 0 18px 44px rgba(0,0,0,.28); animation: pop .3s both; overflow: hidden; }
  .toast svg { width: 16px; height: 16px; color: var(--accent); }
  .toast .accentbar { position: absolute; left: 0; top: 0; bottom: 0; width: 3px; background: var(--accent); }
  @keyframes pop { from { opacity: 0; transform: translate(-50%, 10px); } to { opacity: 1; transform: translate(-50%, 0); } }
</style>
