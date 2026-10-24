(function () {
  if (window.__acU) return;
  var U = {};

  function esc(s) {
    if (window.CSS && CSS.escape) return CSS.escape(s);
    return s.replace(/[^a-zA-Z0-9_-]/g, '\\$&');
  }

  function unique(sel) {
    try { return document.querySelectorAll(sel).length === 1; } catch (e) { return false; }
  }

  U.pathOf = function (el) {
    if (el.id) {
      var own = '#' + esc(el.id);
      if (unique(own)) return own;
    }
    var parts = [];
    var cur = el;
    while (cur && cur.nodeType === 1 && cur !== document.documentElement && cur !== document.body) {
      var part = cur.tagName.toLowerCase();
      if (cur.id) {
        var sid = '#' + esc(cur.id);
        if (unique(sid)) { parts.unshift(sid); break; }
      }
      var cls = Array.prototype.filter.call(cur.classList, function (c) {
        return !/\d{3,}/.test(c);
      }).slice(0, 3);
      if (cls.length) part += '.' + cls.map(esc).join('.');
      var p = cur.parentElement;
      if (p) {
        var same = Array.prototype.filter.call(p.children, function (x) {
          try { return x.matches(part); } catch (e) { return false; }
        });
        if (same.length > 1) {
          part += ':nth-child(' + (Array.prototype.indexOf.call(p.children, cur) + 1) + ')';
        }
      }
      parts.unshift(part);
      if (unique(parts.join(' > '))) break;
      cur = p;
    }
    return parts.join(' > ');
  };

  U.nameOf = function (el, withText) {
    var n = el.tagName.toLowerCase();
    if (el.id) n += '#' + el.id;
    else if (el.classList.length) n += '.' + Array.prototype.slice.call(el.classList, 0, 2).join('.');
    if (withText) {
      var tx = (el.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 16);
      if (tx) n += ' "' + tx + '"';
    }
    return n.slice(0, 60);
  };

  U.hasBg = function (el) {
    var cs = getComputedStyle(el);
    var c = cs.backgroundColor;
    return !!((c && c !== 'transparent' && c !== 'rgba(0, 0, 0, 0)') || (cs.backgroundImage && cs.backgroundImage !== 'none'));
  };

  U.baseCss = function (op) {
    return 'html{background:rgba(0,0,0,' + op + ')!important;background-image:none!important;color-scheme:normal!important}\n' +
      'body{background:transparent!important;background-image:none!important}';
  };

  U.applyBgDirect = function (op) {
    if (document.documentElement) {
      document.documentElement.style.setProperty('background', 'rgba(0,0,0,' + op + ')', 'important');
      document.documentElement.style.setProperty('background-image', 'none', 'important');
    }
    if (document.body) {
      document.body.style.setProperty('background', 'transparent', 'important');
      document.body.style.setProperty('background-image', 'none', 'important');
    }
  };

  U.bgCss = function (list) {
    var css = list.map(function (i) {
      return i.sel + '{background:transparent!important;background-image:none!important}';
    }).join('\n');
    return css + '\n[data-ac-bg]{background:transparent!important;background-image:none!important}';
  };

  var auto = false;
  var timer = null;
  var observing = false;
  var lastRun = 0;
  var quiet = 0;

  function autoScan() {
    timer = null;
    lastRun = Date.now();
    if (!auto || !document.body || document.hidden) return;
    var vw = window.innerWidth;
    var vh = window.innerHeight;
    var found = 0;
    var pts = [[0.5, 0.5], [0.05, 0.05], [0.95, 0.05], [0.05, 0.95], [0.95, 0.95], [0.5, 0.05], [0.5, 0.95]];
    pts.forEach(function (p) {
      var els = document.elementsFromPoint(p[0] * vw, p[1] * vh);
      els.forEach(function (el) {
        if (el === document.documentElement || el === document.body || el.hasAttribute('data-ac-bg')) return;
        var r = el.getBoundingClientRect();
        if (r.width < vw * 0.8 || r.height < vh * 0.8) return;
        if (U.hasBg(el)) {
          el.setAttribute('data-ac-bg', '');
          found++;
        }
      });
    });
    if (found) quiet = 0;
    else if (quiet < 4) quiet++;
  }

  function schedule() {
    if (!auto || timer) return;
    var gap = Math.min(10000, 1500 * (1 << quiet));
    var wait = Math.max(400, gap - (Date.now() - lastRun));
    timer = setTimeout(autoScan, wait);
  }

  function reschedule() {
    quiet = 0;
    schedule();
  }

  function clearMarks() {
    Array.prototype.forEach.call(document.querySelectorAll('[data-ac-bg]'), function (el) {
      el.removeAttribute('data-ac-bg');
    });
  }

  function observe() {
    if (observing || !document.documentElement) return;
    observing = true;
    new MutationObserver(schedule).observe(document.documentElement, { childList: true, subtree: true });
    window.addEventListener('resize', reschedule);
    window.addEventListener('load', reschedule);
    document.addEventListener('DOMContentLoaded', reschedule);
  }

  U.setAuto = function (a) {
    var was = auto;
    auto = !!a;
    observe();
    if (was && !auto) clearMarks();
    if (auto) reschedule();
  };

  U.candidates = function () {
    var vw = window.innerWidth;
    var vh = window.innerHeight;
    var out = [];
    var seen = {};
    var all = document.body ? document.body.querySelectorAll('*') : [];
    var n = Math.min(all.length, 6000);
    for (var i = 0; i < n; i++) {
      var el = all[i];
      var tag = el.tagName;
      if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'LINK' || tag === 'META' || tag === 'IMG' || tag === 'svg') continue;
      var r = el.getBoundingClientRect();
      if (r.width < vw * 0.3 || r.height < vh * 0.3) continue;
      if (!(el.hasAttribute('data-ac-bg') || U.hasBg(el))) continue;
      var sel = U.pathOf(el);
      if (!sel || seen[sel]) continue;
      seen[sel] = 1;
      out.push({
        sel: sel,
        name: (U.nameOf(el, false) + ' ' + Math.round(r.width) + 'x' + Math.round(r.height)).slice(0, 70),
        a: r.width * r.height
      });
    }
    out.sort(function (a, b) { return b.a - a.a; });
    return out.slice(0, 40).map(function (o) { return { sel: o.sel, name: o.name }; });
  };

  window.__acScan = function () {
    try {
      window.ipc.postMessage(JSON.stringify({ t: 'cands', list: U.candidates() }));
    } catch (e) {}
  };

  window.addEventListener('load', function () { setTimeout(window.__acScan, 2500); });

  window.__acU = U;
})();
