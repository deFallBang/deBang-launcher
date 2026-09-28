/* deBang Launcher — лендинг.
   Без зависимостей: прогрессивное улучшение, деградация без JS. */

(() => {
  "use strict";

  /** Репозиторий — единственный источник правды для всех ссылок. */
  const REPO = "deFallBang/deBang-launcher";
  const REPO_URL = `https://github.com/${REPO}`;
  const API = `https://api.github.com/repos/${REPO}`;

  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

  /* ---------------------------------------------------------------- *
   *  Ссылки на репозиторий и релиз
   * ---------------------------------------------------------------- */
  function applyRepoLinks() {
    $$("[data-repo-link]").forEach((a) => {
      a.href = REPO_URL;
    });
    $$("[data-repo-url-text]").forEach((el) => {
      el.textContent = REPO_URL;
    });
    const gh = $("#gh-star-button");
    if (gh) {
      gh.dataset.repo = REPO;
      gh.href = REPO_URL;
    }
  }

  /* ---------------------------------------------------------------- *
   *  Звёзды и последняя версия (GitHub API, без токена).
   *  При лимите или офлайне остаются заглушки — страница работает всегда.
   * ---------------------------------------------------------------- */
  const nf = new Intl.NumberFormat("ru-RU");

  function renderStars(stars) {
    $$("[data-stars]").forEach((el) => {
      el.textContent = nf.format(stars);
    });
  }

  function renderVersion(tag) {
    const pretty = String(tag).replace(/^v/i, "");
    $$("[data-version]").forEach((el) => {
      el.textContent = pretty;
    });
  }

  async function loadGitHubMeta() {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 6000);
    try {
      const res = await fetch(`${API}?x=${Date.now()}`, {
        headers: { Accept: "application/vnd.github+json" },
        signal: controller.signal,
      });
      if (!res.ok) return;
      const data = await res.json();
      if (typeof data.stargazers_count === "number") renderStars(data.stargazers_count);
    } catch {
      /* офлайн или лимит — молча оставляем заглушку */
    } finally {
      clearTimeout(timer);
    }

    try {
      const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
        headers: { Accept: "application/vnd.github+json" },
      });
      if (res.ok) {
        const rel = await res.json();
        if (rel && rel.tag_name) renderVersion(rel.tag_name);
      }
    } catch {
      /* не критично */
    }
  }

  /* ---------------------------------------------------------------- *
   *  Мобильное меню
   * ---------------------------------------------------------------- */
  function initMenu() {
    const toggle = $("#menu-toggle");
    const menu = $("#mobile-menu");
    if (!toggle || !menu) return;
    const open = $("#menu-icon-open");
    const close = $("#menu-icon-close");

    const setOpen = (state) => {
      menu.classList.toggle("hidden", !state);
      open?.classList.toggle("hidden", state);
      close?.classList.toggle("hidden", !state);
      toggle.setAttribute("aria-expanded", String(state));
    };

    toggle.addEventListener("click", () =>
      setOpen(toggle.getAttribute("aria-expanded") !== "true"),
    );
    $$("#mobile-menu a").forEach((a) => a.addEventListener("click", () => setOpen(false)));
    window.addEventListener("keydown", (e) => e.key === "Escape" && setOpen(false));
    window.addEventListener("resize", () => {
      if (window.innerWidth >= 1024) setOpen(false);
    });
  }

  /* ---------------------------------------------------------------- *
   *  Липкий header: фон и граница появляются после скролла
   * ---------------------------------------------------------------- */
  function initHeader() {
    const header = $("#site-header");
    if (!header) return;
    const onScroll = () => {
      const scrolled = window.scrollY > 12;
      header.dataset.scrolled = String(scrolled);
      header.classList.toggle("bg-ink-950/80", scrolled);
      header.classList.toggle("backdrop-blur-xl", scrolled);
      header.classList.toggle("border-b", scrolled);
      header.classList.toggle("border-white/8", scrolled);
    };
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
  }

  /* ---------------------------------------------------------------- *
   *  Появление блоков при скролле
   * ---------------------------------------------------------------- */
  function initReveal() {
    const items = $$("[data-reveal]");
    if (!items.length) return;
    if (!("IntersectionObserver" in window)) {
      items.forEach((el) => el.classList.add("is-visible"));
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          entry.target.classList.add("is-visible");
          io.unobserve(entry.target);
        });
      },
      { rootMargin: "0px 0px -12% 0px", threshold: 0.12 },
    );
    items.forEach((el) => io.observe(el));

    // Всё, что уже видно в первой отрисовке, показываем сразу — иначе
    // первый экран мигнёт пустотой (и останется пустым, если вкладка была
    // в фоне и наблюдатель не сработал).
    requestAnimationFrame(() => {
      items.forEach((el) => {
        if (el.getBoundingClientRect().top < window.innerHeight * 0.95) {
          el.classList.add("is-visible");
          io.unobserve(el);
        }
      });
    });

    // Страховка: что бы ни случилось с наблюдателем, контент обязан появиться.
    window.setTimeout(() => {
      items.forEach((el) => el.classList.add("is-visible"));
    }, 2000);
  }

  /* ---------------------------------------------------------------- *
   *  Подсветка активного раздела в шапке
   * ---------------------------------------------------------------- */
  function initScrollSpy() {
    const links = $$('header nav a[href^="#"]');
    const sections = links
      .map((a) => document.getElementById(a.getAttribute("href").slice(1)))
      .filter(Boolean);
    if (!sections.length || !("IntersectionObserver" in window)) return;

    const spy = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          links.forEach((a) =>
            a.classList.toggle(
              "text-white",
              a.getAttribute("href") === `#${entry.target.id}`,
            ),
          );
        });
      },
      { rootMargin: "-45% 0px -50% 0px" },
    );
    sections.forEach((s) => spy.observe(s));
  }

  /* ---------------------------------------------------------------- *
   *  Копирование команды установки
   * ---------------------------------------------------------------- */
  function initCopy() {
    $$("[data-copy]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const text = btn.dataset.copy;
        const label = $("[data-copy-label]", btn);
        const done = () => {
          if (!label) return;
          const prev = label.textContent;
          label.textContent = "Скопировано";
          setTimeout(() => {
            label.textContent = prev;
          }, 1600);
        };
        try {
          await navigator.clipboard.writeText(text);
          done();
        } catch {
          // Clipboard API может быть недоступен без HTTPS — делаем запасной путь
          const ta = document.createElement("textarea");
          ta.value = text;
          ta.style.position = "fixed";
          ta.style.opacity = "0";
          document.body.appendChild(ta);
          ta.select();
          try {
            document.execCommand("copy");
            done();
          } catch {
            /* молча */
          }
          ta.remove();
        }
      });
    });
  }

  /* ---------------------------------------------------------------- *
   *  Счётчик библиотек в макете окна (лёгкая «живая» деталь)
   * ---------------------------------------------------------------- */
  function initCounters() {
    const el = $("[data-count]");
    if (!el) return;
    const target = Number(el.textContent) || 55;
    if (!("IntersectionObserver" in window)) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        io.disconnect();
        const started = performance.now();
        const step = (now) => {
          const t = Math.min(1, (now - started) / 900);
          el.textContent = String(Math.round(target * (1 - Math.pow(1 - t, 3))));
          if (t < 1) requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      },
      { threshold: 0.4 },
    );
    io.observe(el);
  }

  /* ---------------------------------------------------------------- *
   *  Плавный скролл с учётом фиксированной шапки на старых браузерах
   * ---------------------------------------------------------------- */
  function initSmoothScroll() {
    document.addEventListener("click", (e) => {
      const link = e.target.closest('a[href^="#"]');
      if (!link) return;
      const id = link.getAttribute("href").slice(1);
      const target = id ? document.getElementById(id) : null;
      if (!target) return;
      e.preventDefault();
      const top = target.getBoundingClientRect().top + window.scrollY - 76;
      window.scrollTo({ top, behavior: "smooth" });
      history.replaceState(null, "", id ? `#${id}` : location.pathname);
    });
  }

  function initYear() {
    $$("[data-year]").forEach((el) => {
      el.textContent = String(new Date().getFullYear());
    });
  }

  /* ---------------------------------------------------------------- *
   *  Старт
   * ---------------------------------------------------------------- */
  function init() {
    applyRepoLinks();
    initMenu();
    initHeader();
    initReveal();
    initScrollSpy();
    initCopy();
    initCounters();
    initSmoothScroll();
    initYear();
    loadGitHubMeta();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init, { once: true });
  } else {
    init();
  }
})();
