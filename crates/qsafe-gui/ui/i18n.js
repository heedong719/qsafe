// qsafe-gui i18n loader — vanilla JS, 의존성 0.
//
// 사용법:
//   <script src="i18n.js"></script>
//   await qsafeI18n.init()           // localStorage / navigator.language 자동 감지
//   const s = qsafeI18n.t("toolbar.pack")
//   qsafeI18n.applyDom()             // data-i18n / data-i18n-title 속성 모두 채움
//   await qsafeI18n.setLocale("ja")  // 변경 + localStorage 저장 + applyDom
//   qsafeI18n.available()            // [{ code, name, native }, …]
//   qsafeI18n.current()              // "ko"

(function () {
  "use strict";

  const SUPPORTED = ["ko", "en", "ja", "zh", "es", "fr", "de", "it"];
  const FALLBACK = "en";
  const STORAGE_KEY = "qsafe-locale";

  let bundles = {};          // { ko: {...}, en: {...}, ... }
  let currentLocale = FALLBACK;

  async function fetchLocale(code) {
    if (bundles[code]) return bundles[code];
    try {
      const res = await fetch(`locales/${code}.json`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      bundles[code] = data;
      return data;
    } catch (e) {
      console.warn(`[i18n] failed to load ${code}.json:`, e);
      bundles[code] = {};
      return {};
    }
  }

  function detectInitial() {
    // 1) localStorage
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved && SUPPORTED.includes(saved)) return saved;
    } catch (_) {}
    // 2) navigator.language (ex. ko-KR → ko, zh-CN → zh)
    const nav = (navigator.language || navigator.userLanguage || "en").toLowerCase();
    const code = nav.split("-")[0];
    if (SUPPORTED.includes(code)) return code;
    return FALLBACK;
  }

  function interpolate(template, vars) {
    if (!vars) return template;
    return template.replace(/\{(\w+)\}/g, function (_, k) {
      return vars[k] !== undefined ? String(vars[k]) : "{" + k + "}";
    });
  }

  function t(key, vars) {
    // 우선순위: current → fallback → key 자체
    const cur = bundles[currentLocale] || {};
    if (typeof cur[key] === "string") return interpolate(cur[key], vars);
    const fb = bundles[FALLBACK] || {};
    if (typeof fb[key] === "string") return interpolate(fb[key], vars);
    return key;
  }

  function applyDom(root) {
    const scope = root || document;
    // textContent — data-i18n="key"
    scope.querySelectorAll("[data-i18n]").forEach((el) => {
      const key = el.getAttribute("data-i18n");
      el.textContent = t(key);
    });
    // title — data-i18n-title="key"
    scope.querySelectorAll("[data-i18n-title]").forEach((el) => {
      const key = el.getAttribute("data-i18n-title");
      el.title = t(key);
    });
    // placeholder — data-i18n-placeholder="key"
    scope.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
      const key = el.getAttribute("data-i18n-placeholder");
      el.placeholder = t(key);
    });
    // <title> 갱신
    if (scope === document) {
      const titleKey = document.documentElement.getAttribute("data-i18n-title");
      if (titleKey) document.title = t(titleKey);
      document.documentElement.lang = currentLocale;
    }
  }

  async function init() {
    const initial = detectInitial();
    // fallback과 current 둘 다 미리 로드
    await fetchLocale(FALLBACK);
    if (initial !== FALLBACK) await fetchLocale(initial);
    currentLocale = initial;
    applyDom();
  }

  async function setLocale(code) {
    if (!SUPPORTED.includes(code)) {
      console.warn("[i18n] unsupported locale:", code);
      return;
    }
    await fetchLocale(code);
    currentLocale = code;
    try { localStorage.setItem(STORAGE_KEY, code); } catch (_) {}
    applyDom();
    document.dispatchEvent(new CustomEvent("qsafe-locale-changed", { detail: { locale: code } }));
  }

  async function available() {
    // 모든 SUPPORTED를 fetch해서 _meta 수집 (캐시됨)
    const out = [];
    for (const code of SUPPORTED) {
      const b = await fetchLocale(code);
      const meta = b._meta || {};
      out.push({
        code,
        name: meta.name || code,
        native: meta.native || code,
      });
    }
    return out;
  }

  function current() {
    return currentLocale;
  }

  window.qsafeI18n = { init, setLocale, available, current, t, applyDom };
})();
