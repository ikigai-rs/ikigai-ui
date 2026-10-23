// ikigai-ui's shell script (UI conventions D14): the one first-party script on a page.
//
// It is loaded in <head> WITHOUT defer, before htmx, so the theme is applied before the first
// paint. It touches only <html> while it runs; everything else waits for htmx's events, which
// bubble to `document`.
//
// It does five things htmx does not:
// 1. The theme (D12). `theme=` in the URL wins and is remembered; with none, a remembered choice
//    replaces the server's `auto`. A chooser is a real <select name="theme" data-ui-theme> in a
//    GET form, so it works with no script; with one, a change applies at once.
// 2. Refusals and failures (D14). htmx does not swap a 4xx/5xx answer, so the answer's text is
//    shown, always with textContent and never parsed as markup. A control inside #main gets the
//    note where it is: in place of its own placeholder when it swaps into itself, otherwise right
//    after itself, replacing an earlier note rather than stacking. Anything else goes to #status
//    as an alert. A 403 marks the control aria-disabled (and disables a button): an authority
//    refusal is permanent for this grant. Every other status leaves the control usable.
// 3. The one status region (D13). A request clears #status, except one made by an element
//    inside [data-ui-quiet]: badges, watchers and polls retry on their own, so their requests
//    and their failures are never messages.
// 4. Focus after a swap (D13). A swap into #main moves focus to its first heading (or #main); a
//    swap that took the focused element gives focus to the element with the same id in the new
//    content, else to the swapped region. Focus is never left on <body>.
// 5. Live views (D9). With the door's push stream named in <head>
//    (<meta name="ikigai-ui-push" content="/_ikigai/push">, which ikigai_ui::Pages::push
//    writes), every element that declares what it shows with data-ui-push (whitespace-separated
//    paths, or empty for its own hx-get) is subscribed, one path= each, on ONE EventSource. A
//    `cut` sends "ikigai:cut" to the elements that declared one of its `paths`, and a `resync`
//    sends it to all of them, so a live view says hx-trigger="ikigai:cut" and htmx refetches
//    it. Its own requests are quiet, like [data-ui-quiet]'s, and focus is kept (4). A cut that
//    arrives while the element's request is in flight is not dropped: it is sent again once
//    that request settles. After every swap the declared paths are read again, and the stream
//    is reopened only when they changed. Nothing from the stream is parsed as markup.
(function () {
  "use strict";
  var KEY = "ikigai-ui:theme";
  var THEMES = ["auto", "light", "dark", "high-contrast"];
  var root = document.documentElement;

  // ---- 1. the theme -------------------------------------------------------------------------
  function known(theme) {
    return THEMES.indexOf(theme) >= 0;
  }
  function remembered() {
    try {
      return window.localStorage.getItem(KEY);
    } catch (e) {
      return null;
    }
  }
  function remember(theme) {
    try {
      if (theme === "auto") window.localStorage.removeItem(KEY);
      else window.localStorage.setItem(KEY, theme);
    } catch (e) {
      // Storage refused (a private window, blocked site data): the URL still carries it.
    }
  }
  var asked = null;
  try {
    asked = new URLSearchParams(window.location.search).get("theme");
  } catch (e) {
    asked = null;
  }
  if (asked && known(asked)) {
    // The door renders it already; applying it again covers a page served without it.
    root.setAttribute("data-theme", asked);
    remember(asked);
  } else {
    var kept = remembered();
    if (kept && known(kept) && root.getAttribute("data-theme") === "auto") {
      root.setAttribute("data-theme", kept);
    }
  }
  document.addEventListener("change", function (e) {
    var t = e.target;
    if (!t || !t.matches || !t.matches("select[data-ui-theme]") || !known(t.value)) return;
    root.setAttribute("data-theme", t.value);
    remember(t.value);
  });

  // ---- 2 and 3. refusals, failures and the status region -------------------------------------
  function quiet(elt) {
    if (!elt || !elt.closest) return false;
    // A live view's own request is a refetch, never a message (5).
    return !!(elt.closest("[data-ui-quiet]") || (elt.hasAttribute && elt.hasAttribute("data-ui-push")));
  }
  function status(text, alert) {
    var area = document.getElementById("status");
    if (!area) return;
    area.textContent = "";
    if (!text) return;
    var p = document.createElement("p");
    p.className = "ui-note";
    if (alert) p.setAttribute("role", "alert");
    p.textContent = text;
    area.appendChild(p);
  }
  function noteAt(elt, target, text, code) {
    var main = document.getElementById("main");
    if (!elt || !main || elt === main || !main.contains(elt)) return false;
    var p = document.createElement("p");
    p.className = "ui-note";
    p.setAttribute("role", "status");
    p.textContent = text;
    if (!target || target === elt) {
      elt.textContent = "";
      elt.appendChild(p);
    } else {
      var next = elt.nextElementSibling;
      if (next && next.classList && next.classList.contains("ui-note")) next.replaceWith(p);
      else elt.insertAdjacentElement("afterend", p);
    }
    if (code === 403) {
      elt.setAttribute("aria-disabled", "true");
      if (elt.tagName === "BUTTON") elt.disabled = true;
    }
    return true;
  }
  function fail(detail, text, code) {
    var elt = detail && detail.elt;
    if (quiet(elt)) return;
    if (!noteAt(elt, detail && detail.target, text, code)) status(text, true);
  }
  document.addEventListener("htmx:responseError", function (e) {
    var xhr = e.detail && e.detail.xhr;
    var text = ((xhr && xhr.responseText) || "").trim().slice(0, 500);
    fail(e.detail, text || "The server refused that.", xhr && xhr.status);
  });
  document.addEventListener("htmx:sendError", function (e) {
    fail(e.detail, "The server did not answer.", 0);
  });
  document.addEventListener("htmx:beforeRequest", function (e) {
    var d = e.detail || {};
    if (!quiet(d.elt)) status("", false);
    if (d.target && d.target.setAttribute) d.target.setAttribute("aria-busy", "true");
  });
  document.addEventListener("htmx:afterRequest", function (e) {
    var d = e.detail || {};
    if (d.target && d.target.removeAttribute) d.target.removeAttribute("aria-busy");
  });

  // ---- 4. focus after a swap ------------------------------------------------------------------
  var hadFocus = null;
  function focusable(el) {
    return el.matches("a[href], button, input, select, textarea, [tabindex]");
  }
  function focusOn(el) {
    if (!el) return;
    if (!focusable(el)) el.setAttribute("tabindex", "-1");
    el.focus();
  }
  function lost() {
    var a = document.activeElement;
    return !a || a === document.body || !document.contains(a);
  }
  document.addEventListener("htmx:beforeSwap", function () {
    var a = document.activeElement;
    hadFocus = a && a !== document.body && a.id ? a.id : null;
  });
  document.addEventListener("htmx:afterSettle", function (e) {
    var d = e.detail || {};
    var target = d.target;
    if (!target) return;
    // A quiet element's swap never moves focus on purpose, but it may still have taken it.
    if (target.id === "main" && !quiet(d.elt)) {
      focusOn(target.querySelector("h1, h2") || target);
      return;
    }
    if (!lost()) return;
    var again = hadFocus && document.getElementById(hadFocus);
    var region = document.contains(target) ? target : target.id && document.getElementById(target.id);
    focusOn(again || region || document.getElementById("main"));
  });

  // ---- 5. live views ----------------------------------------------------------------------------
  var stream = null;
  var subscribed = "";
  var pending = {};
  function pushUrl() {
    var meta = document.querySelector('meta[name="ikigai-ui-push"]');
    return meta && meta.getAttribute("content");
  }
  function declared(el) {
    var named = (el.getAttribute("data-ui-push") || "").trim();
    if (!named) named = (el.getAttribute("hx-get") || "").trim();
    return named ? named.split(/\s+/) : [];
  }
  function views() {
    return Array.prototype.slice.call(document.querySelectorAll("[data-ui-push]"));
  }
  function busy(el) {
    return el.classList && el.classList.contains("htmx-request");
  }
  function cut(el) {
    el.dispatchEvent(new CustomEvent("ikigai:cut"));
  }
  // Every view that declared one of `paths` (every view, for null) is refetched now, or once its
  // request in flight settles.
  function refresh(paths) {
    views().forEach(function (el) {
      var mine = declared(el);
      var hit = paths === null || mine.some(function (p) { return paths.indexOf(p) >= 0; });
      if (!hit) return;
      if (busy(el)) mine.forEach(function (p) { pending[p] = true; });
      else cut(el);
    });
  }
  function subscribe() {
    var url = pushUrl();
    var paths = [];
    views().forEach(function (el) {
      declared(el).forEach(function (p) {
        if (paths.indexOf(p) < 0) paths.push(p);
      });
    });
    paths.sort();
    var key = url && paths.length && window.EventSource ? url + "\n" + paths.join("\n") : "";
    if (key === subscribed) return;
    subscribed = key;
    if (stream) stream.close();
    stream = null;
    if (!key) return;
    var query = paths.map(function (p) { return "path=" + encodeURIComponent(p); }).join("&");
    stream = new EventSource(url + (url.indexOf("?") >= 0 ? "&" : "?") + query);
    stream.addEventListener("cut", function (e) {
      var event = null;
      try {
        event = JSON.parse(e.data);
      } catch (err) {
        event = null;
      }
      var named = event && Array.isArray(event.paths) ? event.paths : null;
      // An event this script cannot read is treated as a resync: refetching too much is safe.
      refresh(named);
    });
    stream.addEventListener("resync", function () {
      refresh(null);
    });
  }
  document.addEventListener("DOMContentLoaded", subscribe);
  function flush() {
    var again = Object.keys(pending);
    pending = {};
    if (again.length) refresh(again);
  }
  // A failed request swaps nothing and never settles, so a held cut is also sent after it.
  document.addEventListener("htmx:afterRequest", function () {
    window.setTimeout(flush, 0);
  });
  document.addEventListener("htmx:afterSettle", function () {
    flush();
    subscribe();
  });
  window.addEventListener("pagehide", function () {
    if (stream) stream.close();
    stream = null;
    subscribed = "";
  });
  window.addEventListener("pageshow", function (e) {
    if (e.persisted) subscribe();
  });
})();
