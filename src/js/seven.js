(function () {
  if (window.__ac7Config) return;
  var on = false;
  var map = null;
  var asked = false;
  var tries = 0;
  var retryTimer = 0;
  var watchTimer = 0;
  var refreshTimer = 0;
  var observer = null;
  var styled = false;
  var css = 'img.ac7{height:1.6em;width:auto;vertical-align:middle;display:inline-block;margin:0 1px;pointer-events:auto !important;cursor:default}img.ac7:not([data-ok]){width:1.6em;visibility:hidden}';
  var CHAT_SELECTORS = [
    ".leading-\\[1\\.55\\].font-normal",
    ".chat-line-message-body",
    ".style-scope.yt-live-chat-text-message-renderer",
    ".chat-scrollable-area__message-container",
    ".chat-line__message",
    ".link-fragment",
    "#seventv-message-container",
    ".seventv-chat-list",
    ".seventv-message",
    ".seventv-chat-message-body",
    ".seventv-painted-content",
    ".seventv-overlay",
    "#chatroom",
    "#chatroom-messages",
    ".chatroom-banner",
    ".chat-entry-content",
    ".chat-emote-container",
    ".break-all",
    "seventv-container",
    ".seventv-container"
  ];
  var EXCLUDE = '.chat-entry-username,.chat-message-identity,.chat-line__username,.chat-author__display-name,[data-a-target="chat-message-username"],[data-a-target="chat-line-message-username"],.seventv-chat-user,.seventv-chat-user-username,.seventv-chat-badge,#message-input,[class*="emote-menu"],[class*="emote-picker"],[class*="EmoteMenu"]';
  var CHAT_SEL = '';

  function ready(fn) {
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', fn, { once: true });
    else fn();
  }

  var bridge = !!window.__ac7Bridge;
  var imgData = {};
  var imgWait = {};

  function loadImg(img, url) {
    if (!bridge) { img.src = url; return; }
    var d = imgData[url];
    if (d) { img.src = d; return; }
    if (imgWait[url]) { imgWait[url].push(img); return; }
    imgWait[url] = [img];
    setTimeout(function () {
      if (imgWait[url]) window.__ac7img(url, '');
    }, 10000);
    try { window.ipc.postMessage(JSON.stringify({ t: '7tv_img', url: url })); } catch (e) { window.__ac7img(url, ''); }
  }

  window.__ac7img = function (url, data) {
    var list = imgWait[url];
    delete imgWait[url];
    if (data) imgData[url] = data;
    if (!list) return;
    list.forEach(function (im) { im.src = data || url; });
  };

  function has(name) {
    return Object.prototype.hasOwnProperty.call(map, name);
  }

  function skipNode(n) {
    var p = n.parentElement;
    if (!p) return true;
    var t = p.tagName;
    if (t === 'SCRIPT' || t === 'STYLE' || t === 'TEXTAREA' || t === 'INPUT' || t === 'NOSCRIPT') return true;
    if (p.isContentEditable) return true;
    if (CHAT_SEL && !p.closest(CHAT_SEL)) return true;
    if (p.classList.contains('ac7x') || p.closest(EXCLUDE)) return true;
    return false;
  }

  function convert(node) {
    var txt = node.nodeValue;
    if (!txt || txt === '' || !node.parentNode) return;
    var parts = txt.split(/(\s+)/);
    var hit = false;
    for (var i = 0; i < parts.length; i++) {
      if (parts[i] && has(parts[i])) { hit = true; break; }
    }
    if (!hit) return;
    var frag = document.createDocumentFragment();
    var buf = '';
    parts.forEach(function (p) {
      if (p && has(p)) {
        if (buf) { frag.appendChild(document.createTextNode(buf)); buf = ''; }
        var img = document.createElement('img');
        img.className = 'ac7';
        loadImg(img, map[p]);
        img.alt = '';
        img.title = p;
        img.onload = function () { this.setAttribute('data-ok', '1'); };
        img.setAttribute('data-ac7', p);
        img.onerror = function () {
          if (!this.getAttribute('data-r') && /\/2x\.webp$/.test(this.src)) {
            this.setAttribute('data-r', '1');
            this.src = this.src.replace(/2x\.webp$/, '1x.webp');
            return;
          }
          var txt = this.getAttribute('data-ac7');
          if (txt && this.parentNode) {
            var sp = document.createElement('span');
            sp.className = 'ac7x';
            sp.textContent = txt;
            this.parentNode.replaceChild(sp, this);
          }
        };
        frag.appendChild(img);
      } else {
        buf += p;
      }
    });
    if (buf) frag.appendChild(document.createTextNode(buf));
    node.parentNode.replaceChild(frag, node);
  }

  function walk(root) {
    if (!on || !map || !root) return;
    if (root.nodeType === 3) {
      if (!skipNode(root)) convert(root);
      return;
    }
    if (root.nodeType !== 1) return;
    if (CHAT_SEL && !root.closest(CHAT_SEL) && !root.querySelector(CHAT_SEL)) return;
    var w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
    var list = [];
    while (w.nextNode()) list.push(w.currentNode);
    list.forEach(function (n) { if (!skipNode(n)) convert(n); });
  }

  function revert() {
    Array.prototype.forEach.call(document.querySelectorAll('img.ac7'), function (i) {
      i.replaceWith(document.createTextNode(i.getAttribute('data-ac7') || i.alt));
    });
  }

  function post(data) {
    try {
      window.ipc.postMessage(JSON.stringify(Object.assign({ t: '7tv_id' }, data)));
    } catch (e) {}
  }

  function clearTimers() {
    clearTimeout(retryTimer);
    clearTimeout(watchTimer);
    clearTimeout(refreshTimer);
    retryTimer = 0;
    watchTimer = 0;
    refreshTimer = 0;
  }

  function again() {
    asked = false;
    detect();
  }

  function retry(force) {
    if (!on || (!force && map && Object.keys(map).length) || tries >= 14) return;
    tries++;
    clearTimeout(retryTimer);
    retryTimer = setTimeout(again, Math.min(60000, 2000 * Math.pow(2, Math.min(tries - 1, 5))));
  }

  function watch() {
    clearTimeout(watchTimer);
    watchTimer = setTimeout(function () {
      if (on && !(map && Object.keys(map).length)) retry();
    }, 15000);
  }

  function refresh() {
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(function () {
      if (!on) return;
      if (document.hidden) { refresh(); return; }
      again();
      refresh();
    }, 900000);
  }

  function miss() {
    post(null);
    retry();
  }

  function found(d) {
    post(d);
    watch();
  }

  function detect() {
    if (asked) return;
    asked = true;
    var h = location.hostname;
    if (/(^|\.)kick\.com$/.test(h)) {
      var m = location.pathname.match(/^\/popout\/([^\/]+)\/chat/) || location.pathname.match(/^\/([^\/?#]+)/);
      var slug = m ? m[1] : null;
      if (!slug) { miss(); return; }
      fetch('/api/v2/channels/' + encodeURIComponent(slug), { credentials: 'include' })
        .then(function (r) { return r.json(); })
        .then(function (j) {
          var id = j && (j.user_id || (j.user && j.user.id));
          if (id) found({ platform: 'kick', id: id }); else miss();
        })
        .catch(miss);
    } else if (/(^|\.)twitch\.tv$/.test(h)) {
      var reserved = { directory: 1, videos: 1, settings: 1, downloads: 1, jobs: 1, turbo: 1, search: 1, p: 1, popout: 1, u: 1, subscriptions: 1, wallet: 1, friends: 1, drops: 1, prime: 1 };
      var m = location.pathname.match(/^\/(?:popout|embed)\/([a-z0-9_]{1,25})\/chat/i) || location.pathname.match(/^\/([a-z0-9_]{1,25})(?:\/chat)?\/?$/i);
      var login = m ? m[1].toLowerCase() : null;
      if (login && reserved[login]) login = null;
      if (login) found({ platform: 'twitch', login: login }); else miss();
    } else if (/(^|\.)youtube\.com$/.test(h)) {
      var m = location.pathname.match(/^\/channel\/(UC[\w-]{22})/);
      if (m) {
        found({ platform: 'youtube', id: m[1] });
      } else {
        var params = new URLSearchParams(location.search);
        var v = params.get('v');
        if (v) {
          fetch('/watch?v=' + encodeURIComponent(v), { credentials: 'include' })
            .then(function (r) { return r.text(); })
            .then(function (t) {
              var m = t.match(/"videoDetails":[\s\S]{0,3000}?"channelId":"(UC[\w-]{22})"/);
              if (m) found({ platform: 'youtube', id: m[1] });
              else miss();
            })
            .catch(miss);
        } else if (location.pathname.indexOf('/@') === 0) {
          fetch(location.pathname, { credentials: 'include' })
            .then(function (r) { return r.text(); })
            .then(function (t) {
              var m = t.match(/"channelId":"(UC[\w-]{22})"/);
              if (m) found({ platform: 'youtube', id: m[1] });
              else miss();
            })
            .catch(miss);
        } else {
          post(null);
        }
      }
    } else {
      post(null);
    }
  }

  function start() {
    ready(function () {
      if (!CHAT_SEL) {
        var valid = [];
        var frag = document.createDocumentFragment();
        CHAT_SELECTORS.forEach(function (sel) {
          try {
            frag.querySelector(sel);
            valid.push(sel);
          } catch (e) {}
        });
        CHAT_SEL = valid.join(',');
      }
      if (!styled && document.documentElement) {
        var st = document.createElement('style');
        st.textContent = css;
        document.documentElement.appendChild(st);
        styled = true;
      }
      if (!observer && document.body) {
        observer = new MutationObserver(function (ms) {
          if (!on || !map) return;
          ms.forEach(function (m) {
            Array.prototype.forEach.call(m.addedNodes, walk);
          });
        });
        observer.observe(document.body, { childList: true, subtree: true });
      }
      if (map) walk(document.body);
      else detect();
    });
  }

  var lastInput = null;
  document.addEventListener('focusin', function (e) {
    var t = e.target;
    if (!t || !t.tagName) return;
    if (t.tagName === 'TEXTAREA' || t.isContentEditable || (t.tagName === 'INPUT' && t.type === 'text')) lastInput = t;
  }, true);

  var INPUT_SELS = [
    '[data-testid="chat-input"]',
    '.editor-input[contenteditable="true"]',
    '[data-a-target="chat-input"]',
    '.chat-wysiwyg-input__editor',
    'yt-live-chat-text-input-field-renderer #input',
    '#message-input',
    'textarea',
    '[contenteditable="true"]'
  ];

  function findInput() {
    for (var i = 0; i < INPUT_SELS.length; i++) {
      var el = document.querySelector(INPUT_SELS[i]);
      if (el) return el;
    }
    return lastInput && lastInput.isConnected ? lastInput : null;
  }

  var strip = null;
  var layer = null;
  var stripTimer = 0;
  var HL = typeof CSS !== 'undefined' && CSS.highlights && typeof Highlight !== 'undefined';
  var hlStyled = false;

  var ZW = ['\u200b', '\u200c', '\u200d', '\u2060'];
  var MK_RE = /[\u200b\u200c\u200d\u2060]{4}\u2003\u2003/g;
  var MK_END = /[\u200b\u200c\u200d\u2060]{4}\u2003\u2003[ \u00a0]?$/;
  var LEFT_END = /[\u2003\u200b\u200c\u200d\u2060]+[ \u00a0]?$/;
  var names = [];
  var pendingBtn = null;
  var bsBusy = false;
  var skipClick = false;

  function markFor(name) {
    var i = names.indexOf(name);
    if (i < 0) {
      if (names.length >= 256) return null;
      names.push(name);
      i = names.length - 1;
    }
    return ZW[(i >> 6) & 3] + ZW[(i >> 4) & 3] + ZW[(i >> 2) & 3] + ZW[i & 3] + '\u2003\u2003';
  }

  function decodeMark(m) {
    var v = 0;
    for (var k = 0; k < 4; k++) v = v * 4 + ZW.indexOf(m.charAt(k));
    return names[v] || '';
  }

  function hasMark(el) {
    MK_RE.lastIndex = 0;
    return MK_RE.test(el.textContent || '');
  }

  function expandText(t) {
    return t.replace(MK_RE, function (m, off, str) {
      var n = decodeMark(m);
      if (!n) return '';
      var pre = off > 0 && !/\s/.test(str.charAt(off - 1)) ? ' ' : '';
      var nx = str.charAt(off + m.length);
      var post = nx && !/\s/.test(nx) ? ' ' : '';
      return pre + n + post;
    }).replace(/\u00a0/g, ' ');
  }

  function offsetRange(el, a, b) {
    var w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
    var pos = 0;
    var r = document.createRange();
    var started = false;
    while (w.nextNode()) {
      var n = w.currentNode;
      var l = n.nodeValue.length;
      if (!started && a <= pos + l) { r.setStart(n, a - pos); started = true; }
      if (started && b <= pos + l) { r.setEnd(n, b - pos); return r; }
      pos += l;
    }
    return null;
  }

  function sendAct(el, act) {
    var text = expandText(el.textContent || '');
    var sel = window.getSelection();
    el.focus();
    if (sel) sel.selectAllChildren(el);
    if (!text.trim()) {
      document.execCommand('delete');
      queueStrip();
      return;
    }
    try { window.ipc.postMessage(JSON.stringify({ t: '7tv_send', text: text, act: act })); } catch (e) {}
  }

  window.__ac7After = function () {
    var b = pendingBtn;
    pendingBtn = null;
    if (b && b.isConnected) {
      skipClick = true;
      b.click();
      skipClick = false;
    }
  };

  document.addEventListener('keydown', function (e) {
    if (!on || !map || e.key !== 'Enter' || e.shiftKey || e.ctrlKey || e.altKey || e.isComposing) return;
    var el = document.activeElement;
    if (!el || !el.isContentEditable || !hasMark(el)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    sendAct(el, 'enter');
  }, true);

  document.addEventListener('click', function (e) {
    if (skipClick || !on || !map) return;
    var el = findInput();
    if (!el || !el.isContentEditable || !hasMark(el)) return;
    var btn = e.target && e.target.closest ? e.target.closest('button,[role="button"]') : null;
    if (!btn) return;
    var c = el;
    for (var i = 0; i < 5 && c.parentElement; i++) c = c.parentElement;
    if (!c.contains(btn) || btn.contains(el)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    pendingBtn = btn;
    sendAct(el, 'click');
  }, true);

  function inputText(el) {
    return el.value !== undefined ? el.value : (el.textContent || '');
  }

  function hideStrip() {
    if (strip) strip.style.display = 'none';
    if (layer) layer.textContent = '';
    if (HL) CSS.highlights.delete('ac7e');
  }

  function mkImg(n) {
    var img = document.createElement('img');
    loadImg(img, map[n]);
    img.alt = '';
    img.title = n;
    img.onerror = function () {
      if (!this.getAttribute('data-r') && /\/2x\.webp$/.test(this.src)) {
        this.setAttribute('data-r', '1');
        this.src = this.src.replace(/2x\.webp$/, '1x.webp');
      }
    };
    return img;
  }

  function bgOf(el) {
    while (el && el.nodeType === 1) {
      var c = getComputedStyle(el).backgroundColor;
      if (c && c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') return c;
      el = el.parentElement;
    }
    return '#18181b';
  }

  function inlinePreview(el) {
    if (!hlStyled) {
      hlStyled = true;
      var st = document.createElement('style');
      st.textContent = '::highlight(ac7e){color:transparent !important;-webkit-text-fill-color:transparent !important;text-shadow:none !important}';
      (document.head || document.documentElement).appendChild(st);
    }
    if (!layer) {
      layer = document.createElement('div');
      layer.style.cssText = 'position:fixed;left:0;top:0;width:0;height:0;z-index:2147483646;pointer-events:none';
      document.body.appendChild(layer);
    }
    var ranges = [];
    var items = [];
    var w = document.createTreeWalker(el, NodeFilter.SHOW_TEXT, null);
    while (w.nextNode()) {
      var node = w.currentNode;
      var re = /[\u200b\u200c\u200d\u2060]{4}\u2003\u2003|[^\s\u200b-\u200d\u2060]+/g;
      var m;
      var txt = node.nodeValue;
      while ((m = re.exec(txt))) {
        var isMk = m[0].length === 6;
        var nm = isMk ? decodeMark(m[0]) : m[0];
        if (!nm || !has(nm)) continue;
        var r = document.createRange();
        r.setStart(node, m.index);
        r.setEnd(node, m.index + m[0].length);
        var box = r.getBoundingClientRect();
        if (!box.width || !box.height) continue;
        if (!isMk) ranges.push(r);
        items.push([nm, box, isMk]);
      }
    }
    if (!items.length) { hideStrip(); return false; }
    var h = new Highlight();
    ranges.forEach(function (r) { h.add(r); });
    CSS.highlights.set('ac7e', h);
    var rect = el.getBoundingClientRect();
    var bgc = bgOf(el);
    var frag = document.createDocumentFragment();
    items.forEach(function (it) {
      if (it[1].bottom < rect.top || it[1].top > rect.bottom) return;
      var img = mkImg(it[0]);
      var ih = Math.min(32, Math.max(it[1].height, it[1].width));
      var iy = it[1].top + it[1].height / 2 - ih / 2;
      img.style.cssText = (it[2] ? '' : 'background-color:' + bgc + ';') + 'position:fixed;object-fit:contain;object-position:center;left:' + it[1].left + 'px;top:' + iy + 'px;width:' + it[1].width + 'px;height:' + ih + 'px';
      frag.appendChild(img);
    });
    layer.textContent = '';
    layer.appendChild(frag);
    if (strip) strip.style.display = 'none';
    return true;
  }

  function updateStrip() {
    if (!on || !map) { hideStrip(); return; }
    var el = document.activeElement;
    if (!el || !(el.isContentEditable || el.tagName === 'TEXTAREA')) el = findInput();
    if (!el || !el.isConnected) { hideStrip(); return; }
    if (HL && el.value === undefined) {
      if (inlinePreview(el)) return;
      return;
    }
    var words = inputText(el).split(/[\s\u200b]+/);
    var seen = {};
    var list = [];
    for (var i = 0; i < words.length && list.length < 16; i++) {
      var w = words[i];
      if (w && has(w) && !seen[w]) { seen[w] = 1; list.push(w); }
    }
    if (!list.length) { hideStrip(); return; }
    if (!strip) {
      strip = document.createElement('div');
      strip.style.cssText = 'position:fixed;z-index:2147483646;display:none;flex-wrap:wrap;gap:6px;padding:6px 8px;background:rgba(24,24,27,.96);border:1px solid #3a3a3d;border-radius:8px;pointer-events:none;box-sizing:border-box';
      document.body.appendChild(strip);
    }
    var key = list.join('\n');
    if (strip.getAttribute('data-k') !== key) {
      strip.setAttribute('data-k', key);
      strip.textContent = '';
      list.forEach(function (n) {
        var img = mkImg(n);
        img.style.cssText = 'height:32px;width:auto;display:block';
        strip.appendChild(img);
      });
    }
    var r = el.getBoundingClientRect();
    strip.style.display = 'flex';
    strip.style.left = Math.max(4, r.left) + 'px';
    strip.style.maxWidth = Math.max(60, Math.min(r.width, window.innerWidth - 8)) + 'px';
    strip.style.bottom = Math.max(4, window.innerHeight - r.top + 6) + 'px';
  }

  function queueStrip() {
    clearTimeout(stripTimer);
    stripTimer = setTimeout(updateStrip, 60);
  }

  ['input', 'keyup', 'click', 'focusin', 'focusout'].forEach(function (ev) {
    document.addEventListener(ev, queueStrip, true);
  });
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Enter') setTimeout(updateStrip, 250);
  }, true);

  document.addEventListener('keydown', function (e) {
    if (!on || !map || e.key !== 'Backspace' || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
    var el = document.activeElement;
    if (!el || !el.isContentEditable) return;
    var sel = window.getSelection();
    if (!sel || !sel.rangeCount || !sel.isCollapsed || !el.contains(sel.anchorNode)) return;
    var r = document.createRange();
    r.selectNodeContents(el);
    r.setEnd(sel.anchorNode, sel.anchorOffset);
    var before = r.toString();
    var idx = before.length;
    var start;
    var mk = MK_END.exec(before);
    var lf = mk ? null : LEFT_END.exec(before);
    if (mk) {
      start = idx - mk[0].length;
    } else if (lf) {
      start = idx - lf[0].length;
    } else {
      var m = /([^\s\u200b-\u200d\u2060]+)(\s?)$/.exec(before);
      if (!m || !has(m[1])) return;
      start = idx - m[0].length;
    }
    var rg = offsetRange(el, start, idx);
    if (!rg) return;
    e.preventDefault();
    e.stopPropagation();
    if (bsBusy) return;
    bsBusy = true;
    var want = before.slice(start, idx);
    sel.removeAllRanges();
    sel.addRange(rg);
    var tries = 0;
    function attempt() {
      var cur = window.getSelection();
      if (!cur || cur.isCollapsed || !el.contains(cur.anchorNode)) {
        var again = offsetRange(el, start, idx);
        if (!again) { bsBusy = false; queueStrip(); return; }
        cur.removeAllRanges();
        cur.addRange(again);
      }
      if (tries === 0) {
        if (!document.execCommand('delete')) {
          el.dispatchEvent(new InputEvent('beforeinput', { inputType: 'deleteContentBackward', bubbles: true, cancelable: true }));
        }
      } else {
        try { window.ipc.postMessage(JSON.stringify({ t: '7tv_key', key: 'backspace' })); } catch (er) {}
      }
      tries++;
      setTimeout(function () {
        var txt = el.textContent || '';
        MK_RE.lastIndex = 0;
        var left = MK_RE.test(txt) && txt.indexOf(want.replace(/[ \u00a0]$/, '')) !== -1;
        MK_RE.lastIndex = 0;
        if (left && tries < 2) {
          attempt();
          return;
        }
        bsBusy = false;
        queueStrip();
      }, tries === 1 ? 50 : 120);
    }
    setTimeout(attempt, 40);
    return;
    queueStrip();
  }, true);

  var tip = null;
  document.addEventListener('mouseover', function (e) {
    var t = e.target;
    if (!on || !t || t.tagName !== 'IMG' || !t.classList.contains('ac7')) return;
    var n = t.getAttribute('data-ac7') || t.alt;
    if (!n) return;
    if (!tip) {
      tip = document.createElement('div');
      tip.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;background:#18181b;color:#fff;font:600 12px/1.2 sans-serif;padding:5px 8px;border-radius:6px;border:1px solid #3a3a3d;white-space:nowrap;display:none';
      document.body.appendChild(tip);
    }
    tip.textContent = n;
    var r = t.getBoundingClientRect();
    tip.style.display = 'block';
    var w = tip.offsetWidth;
    var x = Math.min(Math.max(4, r.left + r.width / 2 - w / 2), window.innerWidth - w - 4);
    var y = r.top - tip.offsetHeight - 6;
    if (y < 4) y = r.bottom + 6;
    tip.style.left = x + 'px';
    tip.style.top = y + 'px';
  }, true);
  document.addEventListener('mouseout', function (e) {
    var t = e.target;
    if (tip && t && t.tagName === 'IMG' && t.classList.contains('ac7')) tip.style.display = 'none';
  }, true);
  setInterval(function () { if (on && map && document.hasFocus()) updateStrip(); }, 500);
  window.addEventListener('resize', queueStrip);
  window.addEventListener('scroll', queueStrip, true);

  function len(el) {
    return (el.value !== undefined ? el.value : el.textContent || '').length;
  }

  function caretEnd(el) {
    if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
      var n = el.value.length;
      try { el.setSelectionRange(n, n); } catch (e) {}
      return;
    }
    var sel = window.getSelection();
    if (!sel) return;
    if (sel.rangeCount && el.contains(sel.anchorNode) && sel.anchorNode !== el) return;
    var r = document.createRange();
    r.selectNodeContents(el);
    r.collapse(false);
    sel.removeAllRanges();
    sel.addRange(r);
  }

  window.__ac7Prepare = function (name) {
    var el = findInput();
    if (!el) return 'noinput';
    var had = false;
    if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
      had = document.activeElement === el;
    } else {
      var sel = window.getSelection();
      had = !!(sel && sel.rangeCount && el.contains(sel.anchorNode));
    }
    el.focus();
    if (!had) {
      if (el.tagName === 'TEXTAREA' || el.tagName === 'INPUT') {
        var n = el.value.length;
        try { el.setSelectionRange(n, n); } catch (e) {}
      } else {
        var s2 = window.getSelection();
        var r = document.createRange();
        r.selectNodeContents(el);
        r.collapse(false);
        s2.removeAllRanges();
        s2.addRange(r);
      }
    }
    if (name && on && map && HL && el.isContentEditable && has(name)) {
      var mk = markFor(name);
      if (mk) return 'ok|' + mk + ' ';
    }
    return 'ok|' + (name ? name + ' ' : '');
  };

  function report(r) {
    try { window.ipc.postMessage(JSON.stringify({ t: '7tv_ins', r: r })); } catch (e) {}
  }

  window.__ac7Insert = function (name) {
    var text = name + ' ';
    var el = findInput();
    if (!el) { report('noinput'); return; }
    var isText = el.tagName === 'TEXTAREA' || el.tagName === 'INPUT';
    function content() {
      return isText ? el.value : (el.textContent || '');
    }
    var steps = [
      ['exec', function () { document.execCommand('insertText', false, text); }],
      ['paste', function () {
        var dt = new DataTransfer();
        dt.setData('text/plain', text);
        el.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));
      }],
      ['input', function () {
        el.dispatchEvent(new InputEvent('beforeinput', { inputType: 'insertText', data: text, bubbles: true, cancelable: true, composed: true }));
      }]
    ];
    if (isText) {
      steps.push(['value', function () {
        var proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
        Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, el.value + text);
        el.dispatchEvent(new Event('input', { bubbles: true }));
      }]);
    }
    var i = 0;
    function next() {
      if (i >= steps.length) { report('fail'); return; }
      var s = steps[i++];
      el.focus();
      caretEnd(el);
      var before = content().length;
      try { s[1](); } catch (e) {}
      setTimeout(function () {
        if (content().length > before) { report('ok:' + s[0]); queueStrip(); }
        else next();
      }, 90);
    }
    el.focus();
    setTimeout(next, 40);
  };

  window.__ac7Config = function (v) {
    v = !!v;
    if (v === on) return;
    on = v;
    if (on) start();
    else { clearTimers(); revert(); hideStrip(); }
  };

  window.__ac7tv = function (m, partial) {
    if (!m || typeof m !== 'object') return;
    map = m;
    clearTimeout(retryTimer);
    clearTimeout(watchTimer);
    if (partial) retry(true);
    else tries = 0;
    if (on) { refresh(); if (document.body) walk(document.body); }
    queueStrip();
  };

  window.__ac7retry = function (force) {
    retry(!!force);
  };

  window.addEventListener('online', function () {
    if (!on || (map && Object.keys(map).length)) return;
    tries = 0;
    clearTimeout(retryTimer);
    again();
  });
})();
