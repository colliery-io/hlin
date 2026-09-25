// The probe: the smallest module that exercises the parent end of the bridge.
//
// It does what the specification asks of a module and no more: acts on the
// first `init` and ignores the page's resends, says `ready`, echoes every
// `heartbeat`, and asks its platform one question over `fetch`, again whenever
// it hears its platform changed. It shows everything the shell tells it — the
// context, the theme, whether it is visible, what changed, what it kept across
// an unmount — so a browser test can read the page's side of the bridge off
// this one's.
//
// Buttons make it ask the shell for things, for the browser tests: a
// parameter, a range, a relayed change, a notice, somewhere to go. Two more
// make it misbehave on purpose: one stops it answering heartbeats, which is
// what a wedged module looks like from outside, and one sends more requests
// at once than a frame may have in flight.
//
// And it streams (*Streaming*): "Follow the feed" reads its platform's feed
// as a module should, granting credit as it consumes. For the browser tests,
// `window.probeStreams` opens streams with whatever credit a test chooses,
// pulls and cancels by hand, and reports every `chunk` and `end` it heard, so
// a test can hold a module still and see that the page holds still too.

(function () {
  'use strict';

  var BRIDGE = [1, 0];
  var sent = 0;
  var initialised = false;
  var answering = true;
  var platform = null;
  var panel = null;
  var instance = null;
  // What each outstanding `fetch` was for, by its id.
  var waiting = {};
  var flood = { answered: 0, refused: 0, other: 0 };
  var asked = 0;
  var heard = 0;
  // Streamed fetches, by their id: what was granted and what arrived.
  var streams = {};
  // Whole fetches a test is waiting on, by their id.
  var promised = {};

  function send(type, data, re) {
    sent += 1;
    var message = { bridge: BRIDGE, id: 'm-' + sent, type: type, data: data };
    if (re) {
      message.re = re;
    }
    // `*`, because the parent's origin is not something an opaque origin can
    // name, and nothing here is a secret (REQ-2.2).
    window.parent.postMessage(message, '*');
    return message.id;
  }

  function show(id, text) {
    document.getElementById(id).textContent = text;
  }

  function ask(path, purpose) {
    var id = send('fetch', {
      method: 'GET',
      path: path,
      query: '',
      headers: { accept: 'application/json' },
    });
    waiting[id] = purpose;
  }

  function whoami() {
    asked += 1;
    document.getElementById('whoami').setAttribute('data-asked', String(asked));
    ask('/api/module/whoami', 'whoami');
  }

  function answered(message) {
    if (streams[message.re]) {
      streamAnswered(message);
      return;
    }
    if (promised[message.re]) {
      var resolve = promised[message.re];
      delete promised[message.re];
      resolve(message.data);
      return;
    }
    var purpose = waiting[message.re];
    if (!purpose) {
      return;
    }
    delete waiting[message.re];
    var data = message.data;

    if (purpose === 'flood') {
      if (data.refusal === 'too_many') {
        flood.refused += 1;
      } else if (!data.refusal && data.status === 200) {
        flood.answered += 1;
      } else {
        flood.other += 1;
      }
      show('flood', flood.answered + ' answered, ' + flood.refused + ' refused, ' + flood.other + ' other');
      return;
    }

    if (data.refusal) {
      show('whoami', 'the shell refused: ' + data.refusal);
      return;
    }
    var text = data.body ? new TextDecoder().decode(data.body) : '';
    try {
      var body = JSON.parse(text);
      show('whoami', body.principal + ' on ' + body.platform + ' (' + data.status + ')');
    } catch (error) {
      show('whoami', 'an unreadable answer (' + data.status + ')');
    }
  }

  // -- Streams --

  function openStream(query, options) {
    options = options || {};
    var id = send('fetch', {
      method: options.method || 'GET',
      path: '/api/module/feed',
      query: query || '',
      headers: { accept: 'text/event-stream' },
      stream: true,
      idempotency_key: options.method && options.method !== 'GET' ? 'probe-' + sent : undefined,
    });
    streams[id] = {
      id: id,
      credit: options.credit === undefined ? 4096 : options.credit,
      auto: !!options.auto,
      status: null,
      streaming: null,
      refusal: null,
      seqs: [],
      bytes: 0,
      pulled: 0,
      ended: false,
      error: null,
      text: '',
    };
    return id;
  }

  function pull(id, bytes) {
    var stream = streams[id];
    if (!stream || stream.ended || bytes <= 0) {
      return;
    }
    stream.pulled += bytes;
    send('pull', { re: id, bytes: bytes });
  }

  function streamAnswered(message) {
    var stream = streams[message.re];
    var data = message.data;
    stream.status = data.status;
    stream.streaming = !!data.streaming;
    stream.refusal = data.refusal || null;
    if (!stream.streaming) {
      stream.ended = true;
    } else {
      pull(stream.id, stream.credit);
    }
    showStreams();
  }

  function chunk(data) {
    var stream = streams[data.re];
    if (!stream || !(data.body instanceof ArrayBuffer)) {
      return;
    }
    stream.seqs.push(data.seq);
    stream.bytes += data.body.byteLength;
    // The tail only, so a long stream does not grow the probe without bound.
    stream.text = (stream.text + new TextDecoder().decode(data.body)).slice(-4096);
    if (stream.auto) {
      pull(stream.id, data.body.byteLength);
    }
    showStreams();
  }

  function ended(data) {
    var stream = streams[data.re];
    if (!stream) {
      return;
    }
    stream.ended = true;
    stream.error = data.error || null;
    showStreams();
  }

  function showStreams() {
    var ids = Object.keys(streams);
    var open = ids.filter(function (id) { return !streams[id].ended; }).length;
    var last = ids.length ? streams[ids[ids.length - 1]] : null;
    var ticks = last ? last.text.match(/tick \d+/g) : null;
    show(
      'streams',
      open + ' open' +
        (last ? ', last ' + (last.refusal ? 'refused: ' + last.refusal
          : last.ended ? 'ended' + (last.error ? ': ' + last.error : '')
          : ticks ? 'at ' + ticks[ticks.length - 1] : 'waiting') : ''),
    );
    document.getElementById('streams').setAttribute('data-open', String(open));
  }

  // For the browser tests: everything a stream heard, as plain data.
  window.probeStreams = {
    open: openStream,
    pull: pull,
    cancel: function (id) {
      send('cancel', { re: id });
    },
    get: function (id) {
      return JSON.parse(JSON.stringify(streams[id] || null));
    },
    // What the platform says its feed wrote, asked whole through the shell.
    counts: function (feed) {
      return new Promise(function (resolve) {
        var id = send('fetch', {
          method: 'GET',
          path: '/api/module/feed/' + encodeURIComponent(feed),
          query: '',
          headers: { accept: 'application/json' },
        });
        promised[id] = function (data) {
          if (data.refusal || data.status !== 200 || !data.body) {
            resolve(null);
            return;
          }
          resolve(JSON.parse(new TextDecoder().decode(data.body)));
        };
      });
    },
  };

  // The surface's range, as whole minutes, and this panel's parameters.
  function context(data) {
    var range = data.time_range;
    show('range', range ? Math.round((range.to_millis - range.from_millis) / 60000) + ' minutes' : 'none');
    show('params', JSON.stringify(data.params || {}));
    show('generation', String(data.generation || 0));
  }

  // The chrome's colour roles, applied to this document, so the probe looks
  // like the panel around it whichever pack drew that.
  function theme(data) {
    var tokens = data.tokens || {};
    Object.keys(tokens).forEach(function (name) {
      document.documentElement.style.setProperty(name, tokens[name]);
    });
    document.documentElement.style.colorScheme = data.scheme;
    show('theme', data.scheme + ', ' + Object.keys(tokens).length + ' tokens');
    document.getElementById('theme').setAttribute('data-accent', tokens['--hlin-accent'] || '');
  }

  window.addEventListener('message', function (event) {
    // Only the page that framed us speaks the bridge.
    if (event.source !== window.parent) {
      return;
    }
    var message = event.data;
    if (!message || typeof message !== 'object' || !message.data) {
      return;
    }

    switch (message.type) {
      case 'init':
        // The page resends `init` until it hears `ready`, because it cannot
        // know when this script started listening. Only the first counts.
        if (initialised) {
          return;
        }
        initialised = true;
        platform = message.data.platform;
        panel = message.data.panel;
        instance = message.data.instance;
        document.body.setAttribute('data-instance', instance);
        show('viewer', (message.data.viewer && message.data.viewer.name) || 'someone unnamed');
        context(message.data.context || {});
        theme(message.data.theme || {});
        if (message.data.restored instanceof ArrayBuffer) {
          var kept = new TextDecoder().decode(message.data.restored);
          show('restored', kept);
          document.getElementById('note').value = kept;
        }
        send('ready', { kit: 'hand-written' });
        document.getElementById('state').setAttribute('data-state', 'ready');
        show('state', 'ready');
        whoami();
        break;

      case 'heartbeat':
        if (answering) {
          send('heartbeat', { n: message.data.n }, message.id);
        }
        break;

      case 'response':
        answered(message);
        break;

      case 'chunk':
        chunk(message.data);
        break;

      case 'end':
        ended(message.data);
        break;

      case 'context':
        context(message.data);
        break;

      case 'theme':
        theme(message.data);
        break;

      case 'visibility':
        show('visible', message.data.visible ? 'yes' : 'no');
        break;

      case 'changed':
        heard += 1;
        document.getElementById('changes').setAttribute('data-count', String(heard));
        // Kept per source, because the platform's own events arrive every few
        // seconds and would bury a module's between two looks.
        document
          .getElementById('changes')
          .setAttribute('data-' + message.data.from, message.data.panel);
        show('changes', heard + ', last from ' + message.data.from + ' about ' + message.data.panel);
        // What a module does with news about its own panel: ask again. News
        // about the platform's other panels is heard and shown, and changes
        // nothing here.
        if (message.data.panel === panel) {
          whoami();
        }
        break;

      case 'suspend':
        // About to be unmounted: hand back what a person typed, so it is
        // still there when the frame comes back.
        var note = document.getElementById('note').value;
        send('state', { blob: new TextEncoder().encode(note).buffer }, message.id);
        break;

      default:
        // Anything else, including types from a newer minor, is ignored.
        break;
    }
  });

  document.getElementById('stop').addEventListener('click', function () {
    answering = false;
    document.getElementById('state').setAttribute('data-state', 'silent');
    show('state', 'no longer answering heartbeats');
  });

  document.getElementById('twenty').addEventListener('click', function () {
    flood = { answered: 0, refused: 0, other: 0 };
    for (var i = 0; i < 20; i += 1) {
      ask('/api/module/whoami', 'flood');
    }
  });

  document.getElementById('choose').addEventListener('click', function () {
    send('set-param', { id: 'cluster', values: [platform + '-lab'] });
  });

  document.getElementById('fifteen').addEventListener('click', function () {
    var to = Date.now();
    send('set-range', { from_millis: to - 15 * 60000, to_millis: to });
  });

  document.getElementById('wrote').addEventListener('click', function () {
    send('changed', { panel: panel, selections: {} });
  });

  document.getElementById('notice').addEventListener('click', function () {
    send('notice', {
      level: 'warning',
      text: 'Sync paused.\n<b>Not markup</b> ' + new Array(40).join('and on '),
    });
  });

  document.getElementById('goto').addEventListener('click', function () {
    send('navigate', { to: { platform: platform, panel: 'module-probe' } });
  });

  document.getElementById('follow').addEventListener('click', function () {
    openStream('every_ms=500', { auto: true, credit: 256 * 1024 });
    showStreams();
  });

  document.getElementById('nowhere').addEventListener('click', function () {
    send('navigate', { to: { platform: 'nobody', panel: 'nothing' } });
  });

  // A page, which the shell opens at full width, and a navigation entry that
  // is only a link, which it does not.
  document.getElementById('to-page').addEventListener('click', function () {
    send('navigate', { to: { platform: platform, page: 'module-page' } });
  });

  document.getElementById('to-link').addEventListener('click', function () {
    send('navigate', { to: { platform: platform, page: 'overview' } });
  });
})();
