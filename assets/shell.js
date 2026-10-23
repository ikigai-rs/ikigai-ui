// ikigai-ui's shell script (UI conventions D14): the one first-party script on a page.
//
// It is loaded in <head> WITHOUT defer, before htmx, so the theme is applied before the first
// paint. It touches only <html> while it runs; everything else waits for htmx's events, which
// bubble to `document`.
//
// It does four things htmx does not:
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
    return !!(elt && elt.closest && elt.closest("[data-ui-quiet]"));
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
})();
