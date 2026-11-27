// ikigai-ui's shell script (UI conventions D14): the one first-party script on a page.
//
// It is loaded in <head> WITHOUT defer, before htmx, so the theme is applied before the first
// paint. It touches only <html> while it runs; everything else waits for htmx's events, which
// bubble to `document`.
//
// It does nine things htmx does not (2 includes riding out a restart: a GET that gets a 502,
// 503 or 504 or no answer is sent again, with backoff, before it is shown; see 2a):
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
// 6. Write that lapses (ledger #3822). With the host's session answer named in <head>, a control
//    marked data-ui-write is disabled, with a note saying why, once its `writeSeconds` is 0.
// 7. Workspace tabs (ledger #3950): the ARIA tabs pattern over a [data-ui-tabs] tab list.
// 8. Workspace splitters (ledger #3951): a [data-ui-splitter] separator resized by any pointer,
//    live, and by the keyboard, saved as one `resize` edit through the workspace's "Layout" form.
// 9. Workspace rearrange (ledger #3952): a pane dragged by its heading onto one of five drop
//    targets, a tab dragged along its tab list, and the "Pane" menu's forms, each saved as one
//    `move` or `reorder` edit; the layout then rebuilt around the panes' own elements. The
//    menu's Collapse, Expand, Close and "Open in a new pane", and the "Add pane" form (ledger
//    #3953), are sent the same way; focus goes to the pane the Sink's answer links back to.
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
    // A 403 marks the control aria-disabled and nothing more (ledger #3547): `disabled` would
    // take a button out of the tab order, and with it the refusal its description holds. It stays
    // focusable and announces as unavailable; `inert` below blocks its activation.
    if (code === 403) elt.setAttribute("aria-disabled", "true");
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
  // A refused control (aria-disabled, above) does nothing when activated: a click (which Enter
  // and Space on a button are), a form submit it would make, or any request htmx would send for
  // it. Capture on the document runs before htmx's listener on the control, and before the
  // browser's default (a submit, a link followed). A refused <form> is not inert: its fields stay
  // editable and it may be sent again.
  var CONTROL = "button, a[href], input, select, textarea, summary, [role=button], [role=link], [role=menuitem], [role=tab]";
  function inert(el) {
    var c = el && el.closest ? el.closest(CONTROL) : null;
    return !!c && c.getAttribute("aria-disabled") === "true";
  }
  function block(e) {
    e.preventDefault();
    e.stopImmediatePropagation();
  }
  document.addEventListener("click", function (e) {
    if (inert(e.target)) block(e);
  }, true);
  document.addEventListener("submit", function (e) {
    if (inert(e.submitter)) block(e);
  }, true);
  document.addEventListener("htmx:confirm", function (e) {
    var elt = e.detail && e.detail.elt;
    if (elt && elt.tagName !== "FORM" && inert(elt)) e.preventDefault();
  });
  // ---- 2a. riding out a restart (ledger #3810) ----------------------------------------------
  // A GET that gets a 502, 503 or 504, or no answer at all, is the host restarting or briefly
  // down, so it is sent again rather than shown as a failure: exponential backoff with full
  // jitter (each wait a random part of base x 2^n, capped at 30 s), never sooner than the
  // answer's Retry-After, until the limit has passed since the first failure. Then the last
  // failure is shown as any other. #status says "Reconnecting…" once, politely, while any
  // request is being retried, and is cleared when the last one succeeds. Only a GET is resent:
  // resending a write safely needs an idempotency key the page does not have. Quiet elements
  // retry on their own and are left alone. The limit and base are read from <html>
  // (data-ui-retry-limit, data-ui-retry-base, in milliseconds) at each failure, so a host can
  // make its pages and the faces it frames give up at the same moment.
  var RETRY_LIMIT = 90000;
  var RETRY_BASE = 1000;
  var RETRY_CAP = 30000;
  var RECONNECTING = "Reconnecting\u2026";
  function setting(name, fallback) {
    var raw = root.getAttribute(name);
    if (raw === null || !/^\s*\d+\s*$/.test(raw)) return fallback;
    return parseInt(raw, 10);
  }
  // Retry-After in seconds or as an HTTP date, in milliseconds from now; 0 when absent.
  function retryAfter(xhr) {
    var raw = null;
    try {
      raw = xhr && xhr.getResponseHeader ? xhr.getResponseHeader("Retry-After") : null;
    } catch (err) {
      raw = null;
    }
    if (!raw) return 0;
    raw = raw.trim();
    if (/^\d+$/.test(raw)) return parseInt(raw, 10) * 1000;
    var at = Date.parse(raw);
    return isNaN(at) ? 0 : Math.max(0, at - Date.now());
  }
  function temporary(code) {
    return code === 0 || code === 502 || code === 503 || code === 504;
  }
  function isGet(detail) {
    var rc = detail && detail.requestConfig;
    return !!rc && String(rc.verb || "").toLowerCase() === "get";
  }
  // One session per element being retried: { elt, first, attempt, timer }.
  var sessions = [];
  var bySession = new WeakMap(); // xhr -> the session that sent it
  var resending = null; // the session whose request htmx is issuing right now
  var reconnectNote = null;
  function sessionOf(elt) {
    for (var i = 0; i < sessions.length; i += 1) if (sessions[i].elt === elt) return sessions[i];
    return null;
  }
  function reconnectShown() {
    var area = document.getElementById("status");
    return !!reconnectNote && !!area && area.contains(reconnectNote);
  }
  function endSession(s) {
    if (s.timer) window.clearTimeout(s.timer);
    s.timer = null;
    sessions = sessions.filter(function (x) { return x !== s; });
    if (!sessions.length) {
      // Cleared only if it is still what #status says: anything said since stays.
      if (reconnectShown()) status("", false);
      reconnectNote = null;
    }
  }
  // true when the failure will be retried; false when it is to be shown now.
  function retry(detail, code) {
    var elt = detail && detail.elt;
    if (!elt || quiet(elt) || !temporary(code) || !isGet(detail)) return false;
    var xhr = detail.xhr;
    var s = (xhr && bySession.get(xhr)) || null;
    if (s && sessions.indexOf(s) < 0) s = null; // superseded: this failure starts afresh
    if (!s) {
      var old = sessionOf(elt);
      if (old) endSession(old);
      s = { elt: elt, first: Date.now(), attempt: 0, timer: null };
      sessions.push(s);
    }
    var limit = setting("data-ui-retry-limit", RETRY_LIMIT);
    var base = setting("data-ui-retry-base", RETRY_BASE);
    var remaining = limit - (Date.now() - s.first);
    var floor = retryAfter(xhr);
    if (remaining <= 0 || floor > remaining || !document.contains(elt)) {
      endSession(s);
      return false;
    }
    var ceiling = Math.min(RETRY_CAP, base * Math.pow(2, s.attempt));
    var wait = Math.min(remaining, Math.max(floor, Math.random() * ceiling));
    s.attempt += 1;
    if (!reconnectShown()) {
      status(RECONNECTING, false);
      var area = document.getElementById("status");
      reconnectNote = area ? area.firstElementChild : null;
    }
    var etc = detail.etc || {};
    var rc = detail.requestConfig;
    var target = detail.target && document.contains(detail.target) ? detail.target : undefined;
    s.timer = window.setTimeout(function () {
      s.timer = null;
      if (!document.contains(s.elt)) return endSession(s);
      resending = s;
      try {
        window.htmx.ajax("GET", rc.path, {
          source: s.elt,
          event: rc.triggeringEvent,
          target: target,
          swap: etc.swapOverride,
          select: etc.select,
          headers: etc.headers,
          values: etc.values,
        });
      } catch (err) {
        endSession(s);
      } finally {
        resending = null;
      }
    }, wait);
    return true;
  }
  document.addEventListener("htmx:responseError", function (e) {
    var xhr = e.detail && e.detail.xhr;
    var code = xhr && xhr.status;
    if (retry(e.detail, code)) return;
    var text = ((xhr && xhr.responseText) || "").trim().slice(0, 500);
    fail(e.detail, text || "The server refused that.", code);
  });
  document.addEventListener("htmx:sendError", function (e) {
    if (retry(e.detail, 0)) return;
    fail(e.detail, "The server did not answer.", 0);
  });
  document.addEventListener("htmx:beforeRequest", function (e) {
    var d = e.detail || {};
    if (resending && d.elt === resending.elt) {
      // The same request again: what #status says stays.
      if (d.xhr) bySession.set(d.xhr, resending);
    } else {
      // A new request from an element being retried supersedes the retry.
      var old = d.elt && sessionOf(d.elt);
      if (old) endSession(old);
      if (!quiet(d.elt) && !(sessions.length && reconnectShown())) status("", false);
    }
    if (d.target && d.target.setAttribute) d.target.setAttribute("aria-busy", "true");
  });
  document.addEventListener("htmx:afterRequest", function (e) {
    var d = e.detail || {};
    if (d.target && d.target.removeAttribute) d.target.removeAttribute("aria-busy");
    var s = d.xhr && bySession.get(d.xhr);
    if (s && d.successful && sessions.indexOf(s) >= 0) endSession(s);
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

  // ---- 6. write that lapses (ledger #3822) ------------------------------------------------------
  // With the host's session answer named in <head> (<meta name="ikigai-ui-session"
  // content="/auth/session">, which ikigai_ui::Pages::session writes), the shell reads it once
  // the page has loaded. Its `writeSeconds` is the write left; at 0 every control that needs
  // write, marked with data-ui-write (on the control, or on a form or region around it), is
  // disabled: aria-disabled, so it keeps its name and its place in the tab order, read-only
  // when it is a text field, and described by a visible note beside the marked element that
  // says why and how to reopen it. Activating it, or submitting a marked form, does nothing (as
  // for a 403, 2). A session that never could write (0 at once, with nothing in `restore`) is
  // told so instead. #status says the sentence once, politely. Content swapped in later is
  // marked as it arrives. An answer with no numeric `writeSeconds`, a failed read, or no meta
  // changes nothing. The countdown is held as a deadline, so a page that slept past it lapses
  // as soon as it is shown again.
  var WRITE_LAPSED = "Writing has lapsed for this session. Sign in again to write; you can still read.";
  var WRITE_NEVER = "This session cannot write: its sign-in only reads. Sign in with write access to make changes.";
  var writeOff = null; // the sentence once write is off
  var writeDeadline = 0;
  var writeTimer = null;
  var writeNoteSeq = 0;
  function writeControls(el) {
    if (el.matches(CONTROL)) return [el];
    return Array.prototype.slice.call(el.querySelectorAll(CONTROL));
  }
  function markWrite(scope) {
    if (!writeOff || !scope || !scope.querySelectorAll) return 0;
    var marked = Array.prototype.slice.call(scope.querySelectorAll("[data-ui-write]"));
    if (scope.matches && scope.matches("[data-ui-write]")) marked.unshift(scope);
    var count = 0;
    marked.forEach(function (el) {
      if (el.hasAttribute("data-ui-write-off")) return;
      // A marked element inside another marked one is covered by the outer one's note.
      if (el.parentElement && el.parentElement.closest("[data-ui-write]")) return;
      el.setAttribute("data-ui-write-off", "");
      var p = document.createElement("p");
      p.className = "ui-note";
      writeNoteSeq += 1;
      p.id = "ui-write-note-" + writeNoteSeq;
      p.textContent = writeOff;
      el.insertAdjacentElement("afterend", p);
      writeControls(el).forEach(function (c) {
        c.setAttribute("aria-disabled", "true");
        if (c.matches("textarea, input:not([type]), input[type=text], input[type=email], input[type=search], input[type=url], input[type=tel], input[type=number], input[type=password], input[type=date], input[type=time], input[type=datetime-local], input[type=month], input[type=week]")) {
          c.readOnly = true;
        }
        var ids = (c.getAttribute("aria-describedby") || "").split(/\s+/).filter(Boolean);
        ids.push(p.id);
        c.setAttribute("aria-describedby", ids.join(" "));
        described.push({ control: c, note: p });
      });
      count += 1;
    });
    return count;
  }
  function lapse(sentence) {
    if (writeOff) return;
    writeOff = sentence;
    if (writeTimer) window.clearTimeout(writeTimer);
    writeTimer = null;
    // Once, politely; the notes beside the controls are on screen, so it is for assistive
    // technology. A page with nothing marked has nothing that changed, and says nothing.
    if (markWrite(document)) status(sentence, false, true);
  }
  function writeCheck() {
    if (writeOff || !writeDeadline) return;
    if (writeTimer) window.clearTimeout(writeTimer);
    writeTimer = null;
    var left = writeDeadline - Date.now();
    if (left <= 0) return lapse(WRITE_LAPSED);
    // setTimeout holds at most 2^31 - 1 ms; a longer wait is taken in steps.
    writeTimer = window.setTimeout(writeCheck, Math.min(left, 2147483647));
  }
  function writeAnswer(a) {
    if (!a || typeof a !== "object") return;
    var seconds = a.writeSeconds;
    if (typeof seconds !== "number" || !isFinite(seconds) || seconds < 0) return;
    if (seconds > 0) {
      writeDeadline = Date.now() + seconds * 1000;
      return writeCheck();
    }
    // 0 from the start: lapsed when a fresh sign-in would restore something, else never could.
    lapse(Array.isArray(a.restore) && a.restore.length ? WRITE_LAPSED : WRITE_NEVER);
  }
  function readSession() {
    var meta = document.querySelector('meta[name="ikigai-ui-session"]');
    var url = meta && meta.getAttribute("content");
    if (!url || !window.fetch) return;
    window.fetch(url, { credentials: "same-origin", headers: { Accept: "application/json" } })
      .then(function (r) {
        return r.ok ? r.json() : null;
      })
      .then(writeAnswer, function () {
        // No answer, or not JSON: the host reports nothing, so nothing changes.
      });
  }
  document.addEventListener("DOMContentLoaded", readSession);
  document.addEventListener("htmx:load", function (e) {
    markWrite(e.target);
  });
  // A timer does not run while a machine sleeps: the deadline is checked when the page is seen.
  document.addEventListener("visibilitychange", function () {
    if (document.visibilityState === "visible") writeCheck();
  });
  window.addEventListener("pageshow", writeCheck);
  // A marked form, or a request from inside a marked region, is not sent once write is off.
  document.addEventListener("submit", function (e) {
    if (writeOff && e.target && e.target.closest && e.target.closest("[data-ui-write]")) block(e);
  }, true);
  document.addEventListener("htmx:confirm", function (e) {
    var elt = e.detail && e.detail.elt;
    if (writeOff && elt && elt.closest && elt.closest("[data-ui-write]")) e.preventDefault();
  });
})();

// 7. Workspace tabs (ledger #3950). A [data-ui-tabs] group is a real tab list with no script:
// each tab a link to its pane, the pane shown by :target. With the script it follows the ARIA
// tabs pattern: one tab in the tab order (roving tabindex), Left/Right (and Home/End) move
// between tabs and show the one focused, the others' panels `hidden`, and a link into a hidden
// pane (the "Panes" list) shows its tab first. It changes no state on the server.
(function () {
  "use strict";
  function tabsOf(group) {
    return Array.prototype.slice.call(group.querySelectorAll('[role="tablist"] > [role="tab"]'));
  }
  function show(group, tab, focus) {
    tabsOf(group).forEach(function (t) {
      var on = t === tab;
      t.setAttribute("aria-selected", on ? "true" : "false");
      t.setAttribute("tabindex", on ? "0" : "-1");
      var panel = document.getElementById(t.getAttribute("aria-controls"));
      if (panel) panel.hidden = !on;
    });
    if (focus) tab.focus();
  }
  function ready(root) {
    var groups = root.querySelectorAll ? root.querySelectorAll("[data-ui-tabs]") : [];
    Array.prototype.forEach.call(groups, function (group) {
      if (group.hasAttribute("data-ui-tabs-ready")) return;
      var tabs = tabsOf(group);
      if (!tabs.length) return;
      var active = tabs.filter(function (t) { return t.getAttribute("aria-selected") === "true"; })[0] || tabs[0];
      group.setAttribute("data-ui-tabs-ready", "");
      show(group, active, false);
      group.addEventListener("click", function (e) {
        var tab = e.target.closest && e.target.closest('[role="tab"]');
        if (!tab || !group.contains(tab)) return;
        e.preventDefault();
        show(group, tab, true);
      });
      group.addEventListener("keydown", function (e) {
        var tab = e.target.closest && e.target.closest('[role="tab"]');
        if (!tab) return;
        var all = tabsOf(group);
        var i = all.indexOf(tab);
        var next = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: all.length - 1 }[e.key];
        if (next === undefined) return;
        e.preventDefault();
        show(group, all[(next + all.length) % all.length], true);
      });
    });
  }
  function reveal() {
    var id = decodeURIComponent(location.hash.slice(1));
    var target = id && document.getElementById(id);
    var panel = target && target.closest && target.closest('[role="tabpanel"]');
    var group = panel && panel.closest("[data-ui-tabs-ready]");
    if (!group) return;
    var tab = document.getElementById(panel.getAttribute("aria-labelledby"));
    if (tab) show(group, tab, false);
  }
  document.addEventListener("DOMContentLoaded", function () {
    ready(document);
    reveal();
  });
  document.addEventListener("htmx:load", function (e) {
    ready(e.target);
  });
  window.addEventListener("hashchange", reveal);
})();

// 8. Workspace splitters (ledger #3951, docs/design/workspace.md section 5A "Resize by
// dragging"). The html face puts a role="separator" [data-ui-splitter] between each two children
// of a split, not focusable, with the "Layout" form (a `resize` edit: op, if-version and one
// share{path}.{i} per child) as the way that works with no script. Here, for each splitter whose
// workspace has that form:
// - it joins the tab order. Left/Up and Right/Down move it by 10 thousandths (1%), with Shift by
//   100; Home and End move it to either minimum. Its aria-valuenow (the first child's share),
//   aria-valuemin/max and aria-valuetext ("Dashboard 30%, Session 70%") follow as it moves, so a
//   keyboard resize says nothing in #status. It saves one second after the last key, or when it
//   loses focus.
// - a mouse, a pen or a finger drags it (Pointer Events with pointer capture, not HTML drag and
//   drop, which touch does not have). The two children resize live, once per animation frame,
//   and nothing is composed or refetched. Neither child goes below 16rem wide (8rem tall in a
//   stacked split) nor below a share of 50, and a share moves in steps of 10, so a drag and the
//   keys land on the same shares. The drag saves once, on pointerup, and #status says once
//   "Dashboard is now 30% wide." Escape during a drag, pointercancel, or a capture lost without
//   a pointerup puts it back and saves nothing (SC 2.5.2).
// - a save is the Layout form's own fields, with the new shares, POSTed to its action: the same
//   edit the form makes, one at a time, in order. A save that succeeds says nothing and the form's
//   if-version moves on to the version it wrote; one that fails puts the shares back to the last
//   saved and says why once in #status. A save still waiting when the page is left is sent with
//   keepalive. An aria-disabled splitter does nothing.
(function () {
  "use strict";
  var MIN = 50;
  var STEP = 10;
  var BIG = 100;
  var WAIT = 1000;
  var queue = window.Promise ? Promise.resolve() : null;
  var pending = []; // keyboard saves waiting for their second: { split, timer }
  function children(split) {
    return Array.prototype.filter.call(split.children, function (c) {
      return c.classList.contains("ui-share");
    });
  }
  function splitters(split) {
    return Array.prototype.filter.call(split.children, function (c) {
      return c.hasAttribute("data-ui-splitter");
    });
  }
  // The split's path in the layout ("", ".1", ".1.0"), from the splits around it, as the Layout
  // form names its fields: share{path}.{i}.
  function pathOf(split) {
    var path = "";
    var el = split;
    for (;;) {
      var share = el.parentElement;
      var outer = share && share.parentElement;
      if (!share || !share.classList.contains("ui-share") || !outer || !outer.classList.contains("ui-split")) break;
      path = "." + children(outer).indexOf(share) + path;
      el = outer;
    }
    return path;
  }
  function rootOf(split) {
    var el = split;
    while (el.parentElement && el.parentElement.classList.contains("ui-share") &&
      el.parentElement.parentElement && el.parentElement.parentElement.classList.contains("ui-split")) {
      el = el.parentElement.parentElement;
    }
    return el;
  }
  // The workspace's Layout form: the nearest one around the split with a `resize` op.
  function formOf(split) {
    for (var el = rootOf(split).parentElement; el; el = el.parentElement) {
      var op = el.querySelector('form input[name="op"][value="resize"]');
      if (op && op.form) return op.form;
    }
    return null;
  }
  function field(form, split, i) {
    return form.querySelector('input[name="share' + pathOf(split) + "." + i + '"]');
  }
  function saved(split) {
    var form = formOf(split);
    if (!form) return null;
    var out = [];
    var n = children(split).length;
    for (var i = 0; i < n; i++) {
      var f = field(form, split, i);
      var v = f && parseInt(f.value, 10);
      if (!f || !(v >= MIN)) return null;
      out.push(v);
    }
    return out;
  }
  function percent(v) {
    return (v % 10 === 0 ? v / 10 : (v / 10).toFixed(1)) + "%";
  }
  function names(sep) {
    var label = sep.getAttribute("aria-label") || "";
    var m = /^Resize (.*) and (.*)$/.exec(label);
    return m ? [m[1], m[2]] : ["", ""];
  }
  function row(split) {
    return window.getComputedStyle(split).flexDirection.indexOf("row") === 0;
  }
  // The least share each side of splitter i may keep: 50, or what 16rem (8rem stacked) is of
  // the two children's room, whichever is more.
  function least(split, i, values) {
    var kids = children(split);
    var horizontal = row(split);
    var size = function (el) {
      var r = el.getBoundingClientRect();
      return horizontal ? r.width : r.height;
    };
    var room = size(kids[i]) + size(kids[i + 1]);
    var rem = parseFloat(window.getComputedStyle(document.documentElement).fontSize) || 16;
    var px = (horizontal ? 16 : 8) * rem;
    var total = values[i] + values[i + 1];
    var lo = MIN;
    if (room > 2 * px) lo = Math.max(MIN, Math.ceil((total * px) / room / STEP) * STEP);
    if (lo > total - lo) lo = MIN;
    return { lo: lo, hi: total - lo, room: room, total: total };
  }
  function show(split, values) {
    children(split).forEach(function (kid, i) {
      kid.style.flexGrow = String(values[i]);
    });
    splitters(split).forEach(function (sep) {
      var i = parseInt(sep.getAttribute("data-ui-splitter"), 10);
      var b = least(split, i, values);
      var n = names(sep);
      sep.setAttribute("aria-valuenow", String(values[i]));
      sep.setAttribute("aria-valuemin", String(b.lo));
      sep.setAttribute("aria-valuemax", String(b.hi));
      sep.setAttribute("aria-valuetext", n[0] + " " + percent(values[i]) + ", " + n[1] + " " + percent(values[i + 1]));
    });
  }
  function current(split) {
    return split.uiShares ? split.uiShares.slice() : saved(split);
  }
  // Splitter i at `value`, kept within its bounds; the shares of the two beside it change.
  function moved(split, i, values, value) {
    var b = least(split, i, values);
    var v = Math.min(b.hi, Math.max(b.lo, value));
    var out = values.slice();
    out[i] = v;
    out[i + 1] = b.total - v;
    return out;
  }
  function say(text, seen) {
    var area = document.getElementById("status");
    if (!area) return;
    area.textContent = "";
    if (!text) return;
    var p = document.createElement("p");
    p.className = seen ? "ui-note" : "ui-sr-only";
    p.textContent = text;
    area.appendChild(p);
  }
  function save(split, values, keepalive) {
    var form = formOf(split);
    if (!form || !queue) return;
    var before;
    queue = queue.then(function () {
      before = (split.uiSaved || saved(split) || values).slice();
      // The body is read when its turn comes, so it carries the if-version the save before it
      // moved on to.
      values.forEach(function (v, i) {
        var f = field(form, split, i);
        if (f) f.value = String(v);
      });
      var body = new URLSearchParams(new FormData(form));
      return window.fetch(form.action, {
        method: "POST",
        body: body,
        credentials: "same-origin",
        keepalive: !!keepalive,
        headers: { "HX-Request": "true" },
      }).then(function (r) {
        if (!r.ok) {
          return r.text().then(function (t) {
            throw new Error(t || "the server answered " + r.status);
          });
        }
        split.uiSaved = values.slice();
        var version = form.querySelector('input[name="if-version"]');
        var n = version && parseInt(version.value, 10);
        if (version && n >= 0) version.value = String(n + 1);
      }).catch(function (e) {
        // The screen never shows a layout that is not stored.
        before.forEach(function (v, i) {
          var f = field(form, split, i);
          if (f) f.value = String(v);
        });
        split.uiShares = before.slice();
        show(split, before);
        say("The layout was not saved: " + (e && e.message ? e.message : "no answer") + ".", true);
      });
    });
  }
  function later(split) {
    flush(split, false, true);
    pending.push({
      split: split,
      timer: window.setTimeout(function () {
        flush(split, false);
      }, WAIT),
    });
  }
  // A keyboard save waiting for its second, now (or dropped, with `drop`).
  function flush(split, keepalive, drop) {
    pending = pending.filter(function (p) {
      if (split && p.split !== split) return true;
      window.clearTimeout(p.timer);
      if (!drop) save(p.split, p.split.uiShares, keepalive);
      return false;
    });
  }
  function off(sep) {
    return sep.getAttribute("aria-disabled") === "true" || !!(sep.closest && sep.closest("[data-ui-write-off]"));
  }
  function wire(sep) {
    var split = sep.parentElement;
    var i = parseInt(sep.getAttribute("data-ui-splitter"), 10);
    var drag = null;
    sep.setAttribute("tabindex", "0");
    sep.setAttribute("data-ui-splitter-ready", "");
    function end(keep) {
      if (!drag) return;
      var d = drag;
      drag = null;
      if (d.frame) window.cancelAnimationFrame(d.frame);
      sep.removeAttribute("data-ui-dragging");
      if (sep.hasPointerCapture && sep.hasPointerCapture(d.pointer)) sep.releasePointerCapture(d.pointer);
      if (!keep || d.values[i] === d.start[i]) {
        split.uiShares = d.start;
        show(split, d.start);
        return;
      }
      split.uiShares = d.values;
      show(split, d.values);
      save(split, d.values, false);
      var n = names(sep);
      say(n[0] + " is now " + percent(d.values[i]) + (row(split) ? " wide." : " tall."));
    }
    sep.addEventListener("pointerdown", function (e) {
      if (drag || off(sep) || !e.isPrimary || (e.pointerType === "mouse" && e.button !== 0)) return;
      var values = current(split);
      if (!values) return;
      e.preventDefault();
      flush(split, false);
      sep.focus({ preventScroll: true });
      sep.setPointerCapture(e.pointerId);
      sep.setAttribute("data-ui-dragging", "");
      var b = least(split, i, values);
      drag = {
        pointer: e.pointerId,
        from: row(split) ? e.clientX : e.clientY,
        start: values,
        values: values,
        room: b.room,
        frame: 0,
      };
    });
    sep.addEventListener("pointermove", function (e) {
      if (!drag || e.pointerId !== drag.pointer) return;
      var delta = (row(split) ? e.clientX : e.clientY) - drag.from;
      var raw = drag.start[i] + (drag.room > 0 ? ((drag.start[i] + drag.start[i + 1]) * delta) / drag.room : 0);
      drag.values = moved(split, i, drag.start, Math.round(raw / STEP) * STEP);
      if (!drag.frame) {
        drag.frame = window.requestAnimationFrame(function () {
          if (!drag) return;
          drag.frame = 0;
          show(split, drag.values);
        });
      }
    });
    sep.addEventListener("pointerup", function (e) {
      if (drag && e.pointerId === drag.pointer) end(true);
    });
    sep.addEventListener("pointercancel", function (e) {
      if (drag && e.pointerId === drag.pointer) end(false);
    });
    sep.addEventListener("lostpointercapture", function (e) {
      if (drag && e.pointerId === drag.pointer) end(false);
    });
    sep.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && drag) {
        e.preventDefault();
        return end(false);
      }
      if (drag || off(sep) || e.altKey || e.ctrlKey || e.metaKey) return;
      var values = current(split);
      if (!values) return;
      var step = e.shiftKey ? BIG : STEP;
      var b = least(split, i, values);
      var to = {
        ArrowLeft: values[i] - step,
        ArrowUp: values[i] - step,
        ArrowRight: values[i] + step,
        ArrowDown: values[i] + step,
        Home: b.lo,
        End: b.hi,
      }[e.key];
      if (to === undefined) return;
      e.preventDefault();
      var next = moved(split, i, values, to);
      if (next[i] === values[i]) return;
      split.uiShares = next;
      show(split, next);
      later(split);
    });
    sep.addEventListener("blur", function () {
      flush(split, false);
    });
  }
  function ready(root) {
    var all = root.querySelectorAll ? root.querySelectorAll("[data-ui-splitter]:not([data-ui-splitter-ready])") : [];
    Array.prototype.forEach.call(all, function (sep) {
      var split = sep.parentElement;
      if (!split || !split.classList.contains("ui-split") || !window.fetch) return;
      var values = saved(split);
      if (!values) return;
      wire(sep);
      split.uiSaved = values.slice();
      show(split, values);
    });
  }
  document.addEventListener("DOMContentLoaded", function () {
    ready(document);
  });
  document.addEventListener("htmx:load", function (e) {
    ready(e.target);
  });
  window.addEventListener("pagehide", function () {
    flush(null, true);
  });
})();

// 9. Workspace rearrange (ledger #3952, docs/design/workspace.md section 5A "Rearrange"). The
// html face gives each pane a "Pane" menu (a <details> of edit forms: "Move…", "Put in tabs
// with…", and on a tab "Move tab earlier" and "Move tab later"), each posting one `move` or
// `reorder` edit to the workspace's edit Sink with if-version; with no script the Sink answers a
// page that says what changed and links back. Here:
// - a pane's heading is its drag handle (Pointer Events with capture, so a finger works too).
//   Past a few pixels, the pane under the pointer shows one of five drop targets: left, right,
//   above, below (the outer quarter of the part of it on screen) or into tabs (its middle), outlined and labelled in words ("Right of
//   Dashboard"), and only those its data-ui-drop offers (the server leaves out what would break
//   a bound) and never tabs with a pane of its own tab group. Letting go there sends the `move`
//   edit the "Move…" form would; letting go anywhere else, Escape, or pointercancel sends nothing.
// - a tab dragged along its tab list shows where it would go; letting go sends one `reorder`.
//   A press that does not move is a click, as before.
// - the menu's forms are sent the same way, not by htmx: one edit at a time.
// - an edit that is saved is shown without composing pane content anew on the screen: the page's
//   workspace is fetched again, and each pane's EXISTING element (its content, its live
//   subscription) is moved into the new layout in the DOM, so the reading order and the "Panes"
//   list match the screen. Only its menu and its drop places are taken from the new markup.
//   Focus goes to the moved pane's heading, and #status says the Sink's one sentence (read as
//   text from its answer's [data-ui-said]). An edit that is refused changes nothing on the
//   screen and says why once in #status.
(function () {
  "use strict";
  if (!window.fetch || !window.DOMParser || !window.URLSearchParams) return;
  var THRESHOLD = 6;
  var EDGE = 0.25;
  var busy = false;
  var drag = null;
  var swallowClick = false;
  function workspaceOf(el) {
    for (var at = el; at; at = at.parentElement) {
      if (at.querySelector && at.querySelector(':scope > nav[aria-label="Panes"]')) return at;
    }
    return null;
  }
  function editForm(root) {
    return root && root.querySelector("details[data-ui-pane-menu] form");
  }
  function paneOf(el) {
    return el && el.closest && el.closest("section.ui-pane[id^='pane-']");
  }
  function keyOf(pane) {
    return pane.id.slice(5);
  }
  function labelOf(pane) {
    return pane.getAttribute("aria-label") || keyOf(pane);
  }
  function say(text, seen) {
    var area = document.getElementById("status");
    if (!area) return;
    area.textContent = "";
    if (!text) return;
    var p = document.createElement("p");
    p.className = seen ? "ui-note" : "ui-sr-only";
    p.textContent = text;
    area.appendChild(p);
  }
  // The newest if-version any form of this workspace carries (the splitter moves the Layout
  // form's on as it saves).
  function version(root) {
    var n = 0;
    Array.prototype.forEach.call(root.querySelectorAll('input[name="if-version"]'), function (f) {
      var v = parseInt(f.value, 10);
      if (v > n) n = v;
    });
    return n;
  }
  function heading(key) {
    var h = document.querySelector("#pane-" + key + " > h2");
    if (!h) return null;
    h.setAttribute("tabindex", "-1");
    return h;
  }
  // The page's workspace fetched again, each pane's existing element moved into it.
  function rebuild(root) {
    var url = location.href.split("#")[0];
    return window.fetch(url, { credentials: "same-origin" }).then(function (r) {
      if (!r.ok) throw new Error("the workspace could not be read again (" + r.status + ")");
      return r.text();
    }).then(function (html) {
      var doc = new DOMParser().parseFromString(html, "text/html");
      var nav = doc.querySelector('main nav[aria-label="Panes"]');
      if (!nav) throw new Error("the workspace could not be read again");
      var fresh = document.importNode(nav.parentElement, true);
      Array.prototype.forEach.call(root.querySelectorAll("section.ui-pane[id^='pane-']"), function (old) {
        var next = fresh.querySelector("#" + old.id);
        if (!next) return;
        // Collapsed or expanded (ledger #3953): the new element, which has (or has not) its
        // content; every other pane keeps its own.
        if ((old.getAttribute("data-ui-collapsed") || "false") !== (next.getAttribute("data-ui-collapsed") || "false")) return;
        var menu = old.querySelector(":scope > details[data-ui-pane-menu]");
        var nextMenu = next.querySelector(":scope > details[data-ui-pane-menu]");
        if (menu && nextMenu) menu.replaceWith(nextMenu);
        else if (nextMenu) old.insertBefore(nextMenu, old.children[2] || null);
        // The header toggles too (whether a Collapse is offered depends on the siblings).
        var h2 = old.querySelector(":scope > h2");
        Array.prototype.forEach.call(old.querySelectorAll(":scope > form.ui-pane-toggle"), function (f) { f.remove(); });
        Array.prototype.slice.call(next.querySelectorAll(":scope > form.ui-pane-toggle")).reverse().forEach(function (f) {
          if (h2) h2.after(f);
        });
        old.setAttribute("data-ui-drop", next.getAttribute("data-ui-drop") || "");
        old.removeAttribute("data-ui-dragged");
        next.replaceWith(old);
      });
      root.replaceWith(fresh);
      if (window.htmx && window.htmx.process) window.htmx.process(fresh);
      fresh.dispatchEvent(new CustomEvent("htmx:load", { bubbles: true }));
      return fresh;
    });
  }
  // One edit, then the layout rebuilt, focus on `key`'s heading and the one sentence said.
  function send(root, fields, key) {
    var form = editForm(root);
    if (busy || !form) return;
    busy = true;
    var body = new URLSearchParams();
    Object.keys(fields).forEach(function (k) {
      body.set(k, fields[k]);
    });
    body.set("if-version", String(version(root)));
    var said = "";
    window.fetch(form.action, {
      method: "POST",
      body: body,
      credentials: "same-origin",
      headers: { "HX-Request": "true" },
    }).then(function (r) {
      return r.text().then(function (t) {
        if (!r.ok) throw new Error(t || "the server answered " + r.status);
        var parsed = new DOMParser().parseFromString(t, "text/html");
        var answer = parsed.querySelector("[data-ui-said]");
        said = answer ? answer.textContent : "";
        // The pane focus goes to, as the Sink marks it (after a close, the one that took its
        // place; after an add, the new one): its own attribute, never any pane link.
        var back = parsed.querySelector("[data-ui-focus-pane]");
        if (back) key = back.getAttribute("data-ui-focus-pane") || key;
        return rebuild(root);
      });
    }).then(function (fresh) {
      // After a Collapse or Expand, focus stays on the toggle (now Expand or Collapse, in the
      // same place); after any other edit, on the pane's heading.
      var toggle = (fields.op === "collapse" || fields.op === "expand") &&
        document.querySelector("#pane-" + key + " > form.ui-pane-toggle > button[aria-expanded]");
      if (toggle) {
        toggle.focus();
        say(said, false);
        return;
      }
      var h = heading(key) || (fresh && fresh.querySelector("section.ui-pane > h2"));
      // A pane in a tab that is not shown (the one that took a closed pane's place may be)
      // has its tab shown first, so focus lands on it and never on <body>.
      var panel = h && h.closest('[role="tabpanel"]');
      if (panel && (panel.hidden || panel.classList.contains("ui-tabpanel-inactive"))) {
        var tab = document.getElementById(panel.getAttribute("aria-labelledby") || "");
        if (tab) tab.click();
      }
      if (h) {
        h.setAttribute("tabindex", "-1");
        h.focus({ preventScroll: false });
      }
      say(said, false);
    }).catch(function (e) {
      say("The layout was not changed: " + (e && e.message ? e.message : "no answer") + ".", true);
    }).then(function () {
      busy = false;
    });
  }
  function fieldsOf(form) {
    var out = {};
    new FormData(form).forEach(function (v, k) {
      out[k] = String(v);
    });
    return out;
  }
  // The menu's forms: sent here, not by htmx (capture, before htmx hears the submit).
  document.addEventListener("submit", function (e) {
    var form = e.target;
    var menu = form && form.closest && form.closest("details[data-ui-pane-menu]");
    // The workspace's "Add pane" form, a pane's "Open in a new pane" and its header Collapse or
    // Expand (ledger #3953) are sent the same way; Maximize and Restore are GETs, left to load.
    var other = !menu && form && form.querySelector && (form.method || "").toLowerCase() === "post" &&
      form.querySelector('input[name="op"][value="add"], input[name="op"][value="collapse"], input[name="op"][value="expand"]');
    var root = (menu || other) && workspaceOf(menu || form);
    if (!root) return;
    e.preventDefault();
    e.stopPropagation();
    var fields = fieldsOf(form);
    if (menu) menu.open = false;
    send(root, fields, fields.pane || fields.to || (menu && menu.getAttribute("data-ui-pane-menu")));
  }, true);
  // A page that arrives with a sentence to say (a maximized workspace, ledger #3953) moves focus
  // to its Restore toggle (aria-pressed="true"), then says the sentence once in #status, after
  // the region is in the tree, the way Collapse and Expand do: after a full-page load a screen
  // reader sits on the web content and does not speak a polite change made before focus lands
  // inside it. Only that announced load moves focus; an ordinary load leaves it where it is.
  // It waits for `load`, then a frame: the address ends in #pane-{key}, and scrolling to that
  // fragment runs the browser's focusing steps, which put focus back on the viewport (the pane's
  // section is not focusable) if Restore were focused before it.
  window.addEventListener("load", function () {
    var said = document.querySelector("main [data-ui-announce]");
    if (!said) return;
    window.requestAnimationFrame(function () {
      var restore = document.querySelector('main section.ui-pane > form.ui-pane-toggle > button[aria-pressed="true"]');
      if (restore) restore.focus();
      window.setTimeout(function () { say(said.textContent, false); }, 300);
    });
  });
  // Escape in an open menu closes it and puts focus back on its summary.
  document.addEventListener("keydown", function (e) {
    if (e.key !== "Escape") return;
    if (drag) {
      e.preventDefault();
      return cancel();
    }
    var menu = e.target.closest && e.target.closest("details[data-ui-pane-menu][open]");
    if (!menu) return;
    e.preventDefault();
    menu.open = false;
    menu.querySelector("summary").focus();
  });
  function off(el) {
    return !!(el.closest && el.closest("[data-ui-write-off]"));
  }
  function marker(cls) {
    var m = document.createElement("div");
    m.className = cls;
    m.setAttribute("aria-hidden", "true");
    m.hidden = true;
    document.body.appendChild(m);
    return m;
  }
  // The drop target under (x, y), or null.
  function target(x, y) {
    var under = document.elementFromPoint(x, y);
    var pane = paneOf(under);
    if (!pane || pane === drag.pane || workspaceOf(pane) !== drag.root) return null;
    // The part of the pane on screen: a pane taller than the viewport has its edges there.
    var b = pane.getBoundingClientRect();
    var r = { left: Math.max(b.left, 0), top: Math.max(b.top, 0) };
    r.width = Math.min(b.right, window.innerWidth) - r.left;
    r.height = Math.min(b.bottom, window.innerHeight) - r.top;
    if (r.width <= 0 || r.height <= 0) return null;
    var fx = (x - r.left) / r.width;
    var fy = (y - r.top) / r.height;
    var near = [["left", fx], ["right", 1 - fx], ["above", fy], ["below", 1 - fy]].sort(function (a, b) {
      return a[1] - b[1];
    })[0];
    var place = near[1] < EDGE ? near[0] : "tabs";
    var offered = (pane.getAttribute("data-ui-drop") || "").split(/\s+/);
    if (offered.indexOf(place) < 0) return null;
    var group = drag.pane.closest("[data-ui-tabs]");
    if (place === "tabs" && group && group.contains(pane)) return null;
    var box = { left: r.left, top: r.top, width: r.width, height: r.height };
    if (place === "left" || place === "right") box.width = r.width / 2;
    if (place === "right") box.left = r.left + r.width / 2;
    if (place === "above" || place === "below") box.height = r.height / 2;
    if (place === "below") box.top = r.top + r.height / 2;
    var name = labelOf(pane);
    var words = { left: "Left of ", right: "Right of ", above: "Above ", below: "Below ", tabs: "Into tabs with " }[place] + name;
    return { key: keyOf(pane), place: place, box: box, words: words };
  }
  function show(t) {
    var m = drag.marker;
    if (!t) {
      m.hidden = true;
      return;
    }
    m.hidden = false;
    m.style.left = t.box.left + "px";
    m.style.top = t.box.top + "px";
    m.style.width = t.box.width + "px";
    m.style.height = t.box.height + "px";
    if (!m.firstChild) m.appendChild(document.createElement("span"));
    m.firstChild.textContent = t.words;
  }
  function tabs(list) {
    return Array.prototype.slice.call(list.querySelectorAll(':scope > [role="tab"]'));
  }
  function tabIndex(x) {
    var others = tabs(drag.list).filter(function (t) {
      return t !== drag.handle;
    });
    var i = 0;
    others.forEach(function (t) {
      var r = t.getBoundingClientRect();
      if (x > r.left + r.width / 2) i++;
    });
    var at = others[i] || others[others.length - 1];
    var r = at.getBoundingClientRect();
    return { index: i, x: others[i] ? r.left - 2 : r.right + 2, top: r.top, height: r.height };
  }
  function cleanup() {
    if (!drag) return;
    var d = drag;
    drag = null;
    if (d.marker) d.marker.remove();
    if (d.pane) d.pane.removeAttribute("data-ui-dragged");
    if (d.handle.hasPointerCapture && d.handle.hasPointerCapture(d.pointer)) d.handle.releasePointerCapture(d.pointer);
  }
  function cancel() {
    if (drag && drag.started) swallowClick = drag.kind === "tab";
    cleanup();
  }
  document.addEventListener("pointerdown", function (e) {
    if (drag || busy || !e.isPrimary || (e.pointerType === "mouse" && e.button !== 0)) return;
    var handle = e.target.closest && e.target.closest("[data-ui-handle]");
    var root = handle && workspaceOf(handle);
    if (!root || !editForm(root) || off(handle)) return;
    var kind = handle.getAttribute("role") === "tab" ? "tab" : "pane";
    var pane = kind === "tab" ? document.getElementById("pane-" + handle.id.slice(4)) : paneOf(handle);
    if (!pane) return;
    // No text selection, and no native drag of a tab's link, which would cancel the pointer.
    e.preventDefault();
    drag = {
      kind: kind,
      root: root,
      handle: handle,
      pane: pane,
      list: handle.parentElement,
      pointer: e.pointerId,
      x: e.clientX,
      y: e.clientY,
      started: false,
      at: null,
    };
    handle.setPointerCapture(e.pointerId);
  });
  document.addEventListener("pointermove", function (e) {
    if (!drag || e.pointerId !== drag.pointer) return;
    if (!drag.started) {
      if (Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) < THRESHOLD) return;
      drag.started = true;
      drag.marker = marker(drag.kind === "tab" ? "ui-tab-drop" : "ui-drop");
      if (drag.kind === "pane") drag.pane.setAttribute("data-ui-dragged", "");
    }
    e.preventDefault();
    if (drag.kind === "tab") {
      var t = tabIndex(e.clientX);
      drag.at = t;
      var m = drag.marker;
      m.hidden = false;
      m.style.left = t.x + "px";
      m.style.top = t.top + "px";
      m.style.height = t.height + "px";
      return;
    }
    drag.at = target(e.clientX, e.clientY);
    show(drag.at);
  });
  document.addEventListener("pointerup", function (e) {
    if (!drag || e.pointerId !== drag.pointer) return;
    var d = drag;
    if (!d.started) {
      cleanup();
      return;
    }
    if (d.kind === "tab") swallowClick = true;
    cleanup();
    var key = keyOf(d.pane);
    if (d.kind === "tab") {
      var now = tabs(d.list).indexOf(d.handle);
      if (d.at && d.at.index !== now) send(d.root, { op: "reorder", pane: key, index: String(d.at.index) }, key);
      return;
    }
    if (d.at) send(d.root, { op: "move", pane: key, to: d.at.key, place: d.at.place }, key);
  });
  document.addEventListener("pointercancel", function (e) {
    if (drag && e.pointerId === drag.pointer) cancel();
  });
  document.addEventListener("dragstart", function (e) {
    if (e.target.closest && e.target.closest("[data-ui-handle]")) e.preventDefault();
  });
  // A tab dragged is not also followed as a link.
  document.addEventListener("click", function (e) {
    if (!swallowClick) return;
    swallowClick = false;
    e.preventDefault();
    e.stopPropagation();
  }, true);
  function ready(root) {
    var all = root.querySelectorAll ? root.querySelectorAll("section.ui-pane[id^='pane-'] > h2, [data-ui-tabs] [role='tab']") : [];
    Array.prototype.forEach.call(all, function (h) {
      var ws = workspaceOf(h);
      if (!ws || !editForm(ws)) return;
      h.setAttribute("data-ui-handle", "");
      h.setAttribute("draggable", "false");
    });
  }
  document.addEventListener("DOMContentLoaded", function () {
    ready(document);
  });
  document.addEventListener("htmx:load", function (e) {
    ready(e.target);
  });
})();
