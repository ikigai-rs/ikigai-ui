// ikigai-ui's shell script (UI conventions D14): the one first-party script on a page.
//
// It is loaded in <head> WITHOUT defer, before htmx, so the theme is applied before the first
// paint. It touches only <html> while it runs; everything else waits for htmx's events, which
// bubble to `document`.
//
// It does five things htmx does not:
// 1. The theme (D12). `theme=` in the URL wins and is remembered; with none, a remembered choice
//    replaces the server's `auto`. A chooser is a real <select name="theme" data-ui-theme> in a
//    GET form (the shell renders one in every header), so it works with no script; with one, a
//    change applies at once, <html data-ui-script> hides the form's Apply button, and every
//    chooser on the page is kept showing the theme in force, a remembered one included. The
//    choice is kept in this browser's localStorage only: nothing here sends it anywhere.
// 2. Refusals and failures (D14). htmx does not swap a 4xx/5xx answer, so the answer's text is
//    shown, always with textContent and never parsed as markup. A control inside #main gets the
//    note where it is: in place of its own placeholder when it swaps into itself, otherwise right
//    after itself, replacing an earlier note rather than stacking. The note is plain text with no
//    role and an id; the control points at it with aria-describedby, a link removed when the
//    note is replaced or swapped away. Anything else goes to #status as an alert. A 403 marks
//    the control aria-disabled (and disables a button): an authority refusal is permanent for
//    this grant. Every other status leaves the control usable.
// 3. The one status region (D13). #status is the page's only live region: an in-place note's
//    sentence is said there too (politely, visually hidden, since the note is on screen),
//    prefixed with the control's accessible name. A request clears #status, except one made by
//    an element inside [data-ui-quiet]: badges, watchers and polls retry on their own, so their
//    requests and their failures are never messages.
// 4. Focus after a swap (D13). A swap into #main moves focus to its first heading (or #main); a
//    swap that took the focused element gives focus to the element with the same id in the new
//    content, else to the swapped region, so a swap never leaves focus on <body> after taking it.
//    Focus already on <body> is a page nobody has focused, not focus lost: any other swap there
//    (a poll, a lazily loaded region, a live view) leaves it on <body> and the scroll where it
//    was (ledger #2899).
// 5. Live views (D9). With the door's push stream named in <head>
//    (<meta name="ikigai-ui-push" content="/_ikigai/push">, which ikigai_ui::Pages::push
//    writes), every element that declares what it shows with data-ui-push (whitespace-separated
//    paths, or empty for its own hx-get) is subscribed, one path= each, on ONE EventSource. A
//    `cut` sends "ikigai:cut" to the elements that declared one of its `paths`, and a `resync`
//    sends it to all of them, so a live view says hx-trigger="ikigai:cut" and htmx refetches
//    it. Its own requests are quiet, like [data-ui-quiet]'s, and focus is kept (4). A cut that
//    arrives while the element's request is in flight is not dropped: it is sent again once
//    that request settles. In flight is tracked on the element itself, from htmx:beforeSend to
//    htmx:afterRequest, not only by htmx's htmx-request class, which htmx puts on the
//    INDICATOR (so a view with hx-indicator elsewhere would look idle). After every swap the
//    declared paths are read again, and the stream is reopened only when they changed; a
//    reopened stream refetches every view once its `ready` arrives, because a cut between the
//    old stream closing and the new one listening was heard by nobody. (A new EventSource
//    cannot send Last-Event-ID, and the door answers one with a resync anyway: it keeps no
//    history.) A page back from the back-forward cache is a reopen too: its stream closed at
//    pagehide, so a cut made while it sat in the cache was heard by nobody. A stream the door
//    refuses (a 400 past its subscription bound) is said once in #status; a stream that is
//    only reconnecting says nothing. Nothing from the stream is parsed as markup.
(function () {
  "use strict";
  var KEY = "ikigai-ui:theme";
  var THEMES = ["auto", "light", "dark", "high-contrast"];
  var root = document.documentElement;
  // Before first paint: the token stylesheet hides a chooser's Apply button under it.
  root.setAttribute("data-ui-script", "");

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
  // Every chooser shows the theme in force: the server selected the one it rendered, which a
  // remembered choice may have replaced.
  function showTheme(scope) {
    if (!scope || !scope.querySelectorAll) return;
    var theme = root.getAttribute("data-theme");
    if (!known(theme)) return;
    var list = scope.querySelectorAll("select[data-ui-theme]");
    for (var i = 0; i < list.length; i += 1) {
      if (list[i].value !== theme) list[i].value = theme;
    }
  }
  document.addEventListener("DOMContentLoaded", function () {
    showTheme(document);
  });
  // A swap can bring a chooser in.
  document.addEventListener("htmx:load", function (e) {
    showTheme(e.target && e.target.parentNode);
  });
  document.addEventListener("change", function (e) {
    var t = e.target;
    if (!t || !t.matches || !t.matches("select[data-ui-theme]") || !known(t.value)) return;
    root.setAttribute("data-theme", t.value);
    remember(t.value);
    showTheme(document);
  });

  // ---- 2 and 3. refusals, failures and the status region -------------------------------------
  function quiet(elt) {
    if (!elt || !elt.closest) return false;
    // A live view's own request is a refetch, never a message (5).
    return !!(elt.closest("[data-ui-quiet]") || (elt.hasAttribute && elt.hasAttribute("data-ui-push")));
  }
  function status(text, alert, unseen) {
    var area = document.getElementById("status");
    if (!area) return;
    area.textContent = "";
    if (!text) return;
    var p = document.createElement("p");
    p.className = unseen ? "ui-sr-only" : "ui-note";
    if (alert) p.setAttribute("role", "alert");
    p.textContent = text;
    area.appendChild(p);
  }
  // A note is plain text, never a live region (D13): a region created already holding its text
  // is often not announced, and a second region would say one failure twice. The control
  // points at its note with aria-describedby, so the note is read when focus returns to it, and
  // #status says the sentence once, naming the control.
  var noteSeq = 0;
  var described = [];
  function unlink(control, id) {
    var ids = (control.getAttribute("aria-describedby") || "").split(/\s+/).filter(function (t) {
      return t && t !== id;
    });
    if (ids.length) control.setAttribute("aria-describedby", ids.join(" "));
    else control.removeAttribute("aria-describedby");
  }
  // Every link whose note has left the document (replaced, or swapped away) goes with it.
  function sweep() {
    described = described.filter(function (d) {
      if (document.contains(d.note)) return true;
      unlink(d.control, d.note.id);
      return false;
    });
  }
  function squash(text) {
    return (text || "").replace(/\s+/g, " ").trim();
  }
  // The control's accessible name: aria-labelledby, aria-label, its <label>, or, for a control
  // named by its content, its text. A region with none of these is not named.
  function nameOf(elt) {
    var by = (elt.getAttribute("aria-labelledby") || "").split(/\s+/).filter(Boolean);
    var named = by.map(function (id) {
      var el = document.getElementById(id);
      return el ? squash(el.textContent) : "";
    }).join(" ");
    if (squash(named)) return squash(named);
    if (squash(elt.getAttribute("aria-label"))) return squash(elt.getAttribute("aria-label"));
    if (elt.labels && elt.labels.length) return squash(elt.labels[0].textContent);
    if (elt.matches("button, a[href], summary, [role=button], [role=link], [role=tab], [role=menuitem]")) {
      return squash(elt.textContent);
    }
    return "";
  }
  function noteAt(elt, target, text, code) {
    var main = document.getElementById("main");
    if (!elt || !main || elt === main || !main.contains(elt)) return false;
    var name = nameOf(elt).slice(0, 120);
    var p = document.createElement("p");
    p.className = "ui-note";
    noteSeq += 1;
    p.id = "ui-note-" + noteSeq;
    p.textContent = text;
    if (!target || target === elt) {
      elt.textContent = "";
      elt.appendChild(p);
    } else {
      var next = elt.nextElementSibling;
      if (next && next.classList && next.classList.contains("ui-note")) next.replaceWith(p);
      else elt.insertAdjacentElement("afterend", p);
    }
    sweep();
    var ids = (elt.getAttribute("aria-describedby") || "").split(/\s+/).filter(Boolean);
    ids.push(p.id);
    elt.setAttribute("aria-describedby", ids.join(" "));
    described.push({ control: elt, note: p });
    if (code === 403) {
      elt.setAttribute("aria-disabled", "true");
      if (elt.tagName === "BUTTON") elt.disabled = true;
    }
    // The visible note is beside the control, so the mirror is for assistive technology only.
    status(name ? name + ": " + text : text, false, true);
    return true;
  }
  // A refused form (ledger #3355): an InvalidArgument naming one of the form's fields
  // ("invalid argument `email`: …") lands on that field: its note beside it, aria-invalid, and
  // focus on it, the first field in error. Anything else is the form's note, as for any control.
  function fieldOf(form, text) {
    var m = /invalid argument `([^`]+)`:\s*([\s\S]*)/.exec(text || "");
    if (!m || !form.elements) return null;
    var field = form.elements.namedItem(m[1]);
    if (!field || !field.matches || !field.matches("input, select, textarea")) return null;
    return { field: field, text: m[2] || text };
  }
  function fail(detail, text, code) {
    var elt = detail && detail.elt;
    if (quiet(elt)) return;
    var named = elt && elt.tagName === "FORM" ? fieldOf(elt, text) : null;
    if (named && noteAt(named.field, elt, named.text, code)) {
      named.field.setAttribute("aria-invalid", "true");
      focusOn(named.field);
      return;
    }
    if (elt && elt.tagName === "FORM") {
      if (!noteAt(elt, document.getElementById("status"), text, code)) status(text, true);
      return;
    }
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
  // What had focus as each swap began, by its request (swaps can overlap: a poll's swap must not
  // overwrite what a person's swap found). null when nothing had: focus on <body> is a page
  // nobody has focused yet, never focus lost, so a background poll or a lazily loaded region
  // does not take focus and scroll the page to itself (ledger #2899).
  var hadFocus = new WeakMap();
  var lastHad = null;
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
  document.addEventListener("htmx:beforeSwap", function (e) {
    var d = e.detail || {};
    var a = document.activeElement;
    lastHad = a && a !== document.body && document.contains(a) ? a : null;
    if (d.xhr) hadFocus.set(d.xhr, lastHad);
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
    var had = d.xhr && hadFocus.has(d.xhr) ? hadFocus.get(d.xhr) : lastHad;
    // Only a swap that took the element that HAD focus re-homes it.
    if (!had || !lost()) return;
    var again = had.id && document.getElementById(had.id);
    var region = document.contains(target) ? target : target.id && document.getElementById(target.id);
    focusOn(again || region || document.getElementById("main"));
  });

  // ---- 5. live views ----------------------------------------------------------------------------
  var stream = null;
  var subscribed = "";
  // The live views a cut reached while their own request was in flight: [{ el, paths }].
  var pending = [];
  // In flight, kept on the element itself (htmx's htmx-request class goes on the INDICATOR).
  var inFlight = new WeakSet();
  var sentBy = new WeakMap();
  var refusalSaid = false;
  // Set at pagehide when a stream was open: if the page comes back from the back-forward cache,
  // its next stream is a reopen.
  var away = false;
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
    return inFlight.has(el);
  }
  // beforeSend, not beforeRequest: a request another handler cancels is never sent and never
  // reaches afterRequest, so marking it at beforeRequest would hold its cuts forever.
  document.addEventListener("htmx:beforeSend", function (e) {
    var d = e.detail || {};
    if (!d.elt || !d.elt.hasAttribute || !d.elt.hasAttribute("data-ui-push")) return;
    inFlight.add(d.elt);
    if (d.xhr) sentBy.set(d.xhr, d.elt);
  });
  // afterRequest names the element only while it is in the document (after an outerHTML swap
  // it names the parent, measured), so the request is matched by its xhr.
  document.addEventListener("htmx:afterRequest", function (e) {
    var d = e.detail || {};
    var elt = (d.xhr && sentBy.get(d.xhr)) || d.elt;
    if (elt) inFlight["delete"](elt);
  });
  function cut(el) {
    el.dispatchEvent(new CustomEvent("ikigai:cut"));
  }
  // Every view that declared one of `paths` (every view, for null) is refetched now, or once its
  // request in flight settles. What is held is the VIEW, not its paths: holding paths would
  // send the cut again to every other view on them, whose refetch would hold it again, a loop
  // for as long as the request is out.
  function refresh(paths) {
    views().forEach(function (el) {
      var mine = declared(el);
      var hit = paths === null || mine.some(function (p) { return paths.indexOf(p) >= 0; });
      if (!hit) return;
      if (busy(el)) hold(el, mine);
      else cut(el);
    });
  }
  function hold(el, paths) {
    for (var i = 0; i < pending.length; i += 1) if (pending[i].el === el) return;
    pending.push({ el: el, paths: paths });
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
    var back = away;
    away = false;
    if (key === subscribed) return;
    subscribed = key;
    var reopened = !!stream || back;
    if (stream) stream.close();
    stream = null;
    if (!key) return;
    var query = paths.map(function (p) { return "path=" + encodeURIComponent(p); }).join("&");
    var opened = new EventSource(url + (url.indexOf("?") >= 0 ? "&" : "?") + query);
    stream = opened;
    if (reopened) {
      // The gap between the old stream and this one ends at this one's first `ready`.
      var gap = function () {
        opened.removeEventListener("ready", gap);
        if (stream === opened) refresh(null);
      };
      opened.addEventListener("ready", gap);
    }
    opened.addEventListener("error", function () {
      // CLOSED is EventSource giving up (a refusal: a 4xx, a wrong media type); CONNECTING is
      // an ordinary reconnect, which is quiet.
      if (opened.readyState !== 2 || stream !== opened || refusalSaid) return;
      refusalSaid = true;
      status("Live updates are off: the server refused this page's update stream. Reload to see changes.", false);
    });
    opened.addEventListener("cut", function (e) {
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
    opened.addEventListener("resync", function () {
      refresh(null);
    });
  }
  document.addEventListener("DOMContentLoaded", subscribe);
  // Send each held cut whose request is done. A view still in the document gets it now. A view
  // its own swap replaced gets it through its replacement (the element with its id), and only
  // once THAT swap has settled (`settled` names its target): before then htmx has not wired the
  // new element, and an event sent to it is lost (measured). A replaced view with nothing of
  // its id left in the document is dropped; its content is gone. So a live view that swaps
  // outerHTML wants an id.
  function flush(settled) {
    var held = pending;
    pending = [];
    held.forEach(function (h) {
      if (busy(h.el)) return pending.push(h);
      if (document.contains(h.el)) return cut(h.el);
      if (!h.el.id) {
        // Without an id the replacement cannot be told apart: after a settle, its paths are
        // refetched, which may refetch a neighbor too. Refetching too much is safe.
        if (!settled) return pending.push(h);
        return refresh(h.paths);
      }
      var next = document.getElementById(h.el.id);
      if (!next) return;
      if (!settled || (settled !== h.el && settled.id !== h.el.id)) return pending.push(h);
      if (next.hasAttribute("data-ui-push")) cut(next);
    });
  }
  // A failed request swaps nothing and never settles, so a held cut is also sent after it.
  document.addEventListener("htmx:afterRequest", function () {
    window.setTimeout(function () {
      flush(null);
    }, 0);
  });
  document.addEventListener("htmx:afterSettle", function (e) {
    sweep();
    flush((e.detail && e.detail.target) || null);
    subscribe();
  });
  window.addEventListener("pagehide", function () {
    if (stream) stream.close();
    away = !!stream;
    stream = null;
    subscribed = "";
  });
  window.addEventListener("pageshow", function (e) {
    if (e.persisted) subscribe();
  });
})();
