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

  U.isBox = function (el) {
    if (!el || el.nodeType !== 1) return true;
    if (el === document.body || el === document.documentElement) return true;
    if (el.querySelector('textarea,input,select,[contenteditable]:not([contenteditable="false"])')) return true;
    var cs = getComputedStyle(el);
    if (cs.overflowY === 'auto' || cs.overflowY === 'scroll') return true;
    return el.getBoundingClientRect().height > window.innerHeight * 0.5;
  };

  function classesOf(el, max) {
    var skipCls = /^(active|selected|hover|focus|highlight|highlighted|new|odd|even|first|last|read|unread|pinned|deleted|hidden|visible|show|open|closed)$/i;
    return Array.prototype.filter.call(el.classList, function (c) {
      return !/\d{3,}/.test(c) && !skipCls.test(c) && /^[a-zA-Z_-][a-zA-Z0-9_-]*$/.test(c);
    }).slice(0, max);
  }

  function lowestBlock(el) {
    var cur = el;
    while (cur && cur.nodeType === 1 && cur !== document.body && cur !== document.documentElement) {
      var d = getComputedStyle(cur).display;
      if (d !== 'inline' && d !== 'contents' && d !== 'inline-block' && d !== 'ruby' && d !== 'inline-flex') return cur;
      cur = cur.parentElement;
    }
    return el;
  }

  function validSel(sel, row) {
    var found;
    try { found = document.querySelectorAll(sel); } catch (e) { return 0; }
    if (!found.length || Array.prototype.indexOf.call(found, row) < 0) return 0;
    for (var i = 0; i < found.length; i++) {
      if (U.isBox(found[i])) return 0;
    }
    return found.length;
  }

  U.rowOf = function (el) {
    var row = lowestBlock(el);
    for (var c = el; c && c !== document.body && !U.isBox(c); c = c.parentElement) {
      if (c.tagName.indexOf('-') > 0) row = c;
    }
    if (U.isBox(row)) row = el;
    for (var guard = 0; guard < 6; guard++) {
      var p = row.parentElement;
      if (!p || U.isBox(p) || p.children.length !== 1 || !p.className || typeof p.className !== 'string') break;
      var g = p.parentElement;
      if (!g) break;
      var sibs = Array.prototype.filter.call(g.children, function (x) { return x !== p && x.className === p.className; });
      if (!sibs.length) break;
      row = p;
    }
    var tag = row.tagName.toLowerCase();
    var same = row.parentElement ? Array.prototype.filter.call(row.parentElement.children, function (x) { return x.tagName === row.tagName; }) : [row];
    var cls = classesOf(row, 4);
    if (same.length >= 2) {
      var shared = cls.filter(function (c) {
        var n = 0;
        for (var i = 0; i < same.length; i++) if (same[i].classList.contains(c)) n++;
        return n >= same.length * 0.8;
      });
      if (shared.length) cls = shared;
    }
    var attr = '';
    if (!cls.length) {
      var names = Array.prototype.map.call(row.attributes, function (a) { return a.name; }).filter(function (n) {
        return n.indexOf('data-') === 0 && /^[a-z0-9-]+$/.test(n);
      });
      if (names.length) attr = '[' + names[0] + ']';
    }
    var own = tag + cls.map(function (c) { return '.' + esc(c); }).join('') + attr;
    var specific = cls.length > 0 || attr !== '' || tag.indexOf('-') > 0;
    var cands = [];
    if (specific) cands.push(own);
    var par = row.parentElement;
    if (par && par !== document.body) {
      var pc = classesOf(par, 3);
      var ps = par.id && unique('#' + esc(par.id)) ? '#' + esc(par.id) : par.tagName.toLowerCase() + pc.map(function (c) { return '.' + esc(c); }).join('');
      cands.push(ps + ' > ' + own);
      var pp = U.pathOf(par);
      if (pp) cands.push(pp + ' > ' + own);
    }
    for (var k = 0; k < cands.length; k++) {
      var n2 = validSel(cands[k], row);
      if (n2) return { el: row, sel: cands[k], count: n2 };
    }
    return { el: row, sel: U.pathOf(row), count: 1 };
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
  var scrollbarSheet = null;
  var scrollbarStyle = null;

  function applyScrollbarHider() {
    if (!auto) {
      if (scrollbarSheet && document.adoptedStyleSheets) {
        try {
          document.adoptedStyleSheets = document.adoptedStyleSheets.filter(function (s) { return s !== scrollbarSheet; });
        } catch (e) {}
      }
      if (scrollbarStyle && scrollbarStyle.parentNode) scrollbarStyle.parentNode.removeChild(scrollbarStyle);
      return;
    }
    try {
      if (!scrollbarSheet) {
        scrollbarSheet = new CSSStyleSheet();
        scrollbarSheet.replaceSync('*{scrollbar-width:none!important}*::-webkit-scrollbar{display:none!important;width:0!important;height:0!important}');
      }
      if (document.documentElement) {
        document.adoptedStyleSheets = document.adoptedStyleSheets.concat([scrollbarSheet]);
      }
    } catch (e) {
      if (!scrollbarStyle) {
        scrollbarStyle = document.createElement('style');
        scrollbarStyle.textContent = '*{scrollbar-width:none!important}*::-webkit-scrollbar{display:none!important;width:0!important;height:0!important}';
      }
      if (document.documentElement && !scrollbarStyle.parentNode) {
        document.documentElement.appendChild(scrollbarStyle);
      }
    }
  }

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
    if (auto) applyScrollbarHider();
  }

  U.setAuto = function (a) {
    var was = auto;
    auto = !!a;
    observe();
    if (was && !auto) clearMarks();
    if (auto) reschedule();
    applyScrollbarHider();
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
