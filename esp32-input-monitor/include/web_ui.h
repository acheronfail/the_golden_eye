#pragma once

#include <Arduino.h>

/**
 * Self-contained UI served at "/". The page opens a WebSocket to "/ws" and
 * receives a 4-byte binary frame on every state change:
 *
 *   byte 0 (MSB-first): A B Z START UP DOWN LEFT RIGHT
 *   byte 1 (MSB-first): - - L R C-UP C-DOWN C-LEFT C-RIGHT   (top 2 bits unused)
 *   byte 2: stick X (int8, +right)
 *   byte 3: stick Y (int8, +up/forward)
 *
 * The bit masks below must stay in sync with packState() in main.cpp.
 */
static const char INDEX_HTML[] PROGMEM = R"HTML(<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>N64 Spy</title>
<style>
  :root { color-scheme: dark; }
  body {
    margin: 0; min-height: 100vh; display: grid; place-items: center;
    background: #14161a; color: #c8ccd4;
    font: 14px/1.4 system-ui, -apple-system, Segoe UI, Roboto, sans-serif;
  }
  .wrap { text-align: center; }
  h1 { font-size: 15px; font-weight: 600; letter-spacing: .04em; color: #8a909c; margin: 0 0 4px; }
  #conn { font-size: 12px; color: #6b7280; margin-bottom: 18px; }
  #conn.up { color: #4ade80; }
  #conn.down { color: #f87171; }

  /* Controller layout: three columns (D-pad | center | A/B + C-cluster). */
  .pad {
    display: grid; grid-template-columns: 150px 170px 150px; gap: 18px;
    align-items: start; justify-items: center;
    padding: 28px 30px; border-radius: 22px;
    background: #1d2026; box-shadow: 0 10px 40px rgba(0,0,0,.45), inset 0 1px 0 rgba(255,255,255,.04);
  }
  .col { display: grid; gap: 14px; justify-items: center; }

  /* Generic button indicator. */
  .btn {
    display: grid; place-items: center;
    border-radius: 8px; font-size: 12px; font-weight: 700; letter-spacing: .03em;
    background: #2a2e37; color: #7a8089; border: 1px solid #353a44;
    transition: background .04s, color .04s, box-shadow .04s; user-select: none;
  }
  .btn.on { color: #0f1115; box-shadow: 0 0 14px currentColor; }

  .wide { width: 96px; height: 30px; }
  .round { width: 46px; height: 46px; border-radius: 50%; }
  .small { width: 30px; height: 30px; }

  /* Color themes per the real controller. */
  .blue.on   { background: #4f7bd6; color: #4f7bd6; }   /* A */
  .green.on  { background: #4ade80; color: #4ade80; }   /* B */
  .grey.on   { background: #c8ccd4; color: #c8ccd4; }   /* Z, L, R, Start, D-pad */
  .yellow.on { background: #f5c542; color: #f5c542; }   /* C buttons */

  /* D-pad cross. */
  .dpad { display: grid; grid-template-columns: repeat(3, 30px); grid-template-rows: repeat(3, 30px); gap: 2px; }
  .dpad .up    { grid-area: 1 / 2; }
  .dpad .left  { grid-area: 2 / 1; }
  .dpad .right { grid-area: 2 / 3; }
  .dpad .down  { grid-area: 3 / 2; }

  /* C-button cluster (same cross shape). */
  .cpad { display: grid; grid-template-columns: repeat(3, 30px); grid-template-rows: repeat(3, 30px); gap: 2px; }
  .cpad .cu { grid-area: 1 / 2; }
  .cpad .cl { grid-area: 2 / 1; }
  .cpad .cr { grid-area: 2 / 3; }
  .cpad .cd { grid-area: 3 / 2; }

  /* Analog stick well + moving dot. */
  .stick { position: relative; width: 110px; height: 110px; border-radius: 50%;
           background: radial-gradient(circle at 50% 50%, #23272f, #15171c); border: 1px solid #353a44; }
  .stick .dot { position: absolute; left: 50%; top: 50%; width: 26px; height: 26px; margin: -13px 0 0 -13px;
                border-radius: 50%; background: #c8ccd4; box-shadow: 0 0 10px rgba(0,0,0,.5);
                transition: transform .03s linear; }
  .axis { font-variant-numeric: tabular-nums; color: #6b7280; font-size: 12px; }

  .shoulders { display: flex; gap: 60px; }
  .ab { display: flex; gap: 16px; align-items: center; }
</style>
</head>
<body>
<div class="wrap">
  <h1>N64 SPY</h1>
  <div id="conn">connecting…</div>

  <div class="pad">
    <!-- Left column: shoulders, D-pad -->
    <div class="col">
      <div id="L" class="btn wide grey">L</div>
      <div class="dpad">
        <div id="UP"    class="btn small grey up"></div>
        <div id="LEFT"  class="btn small grey left"></div>
        <div id="RIGHT" class="btn small grey right"></div>
        <div id="DOWN"  class="btn small grey down"></div>
      </div>
    </div>

    <!-- Center column: Start, Z, analog stick -->
    <div class="col">
      <div id="START" class="btn wide grey">START</div>
      <div id="Z" class="btn wide grey">Z</div>
      <div class="stick"><div id="dot" class="dot"></div></div>
      <div class="axis">x:<span id="sx">0</span> y:<span id="sy">0</span></div>
    </div>

    <!-- Right column: shoulder, A/B, C cluster -->
    <div class="col">
      <div id="R" class="btn wide grey">R</div>
      <div class="ab">
        <div id="B" class="btn round green">B</div>
        <div id="A" class="btn round blue">A</div>
      </div>
      <div class="cpad">
        <div id="CUP"    class="btn small yellow cu">C&#9650;</div>
        <div id="CLEFT"  class="btn small yellow cl">C&#9664;</div>
        <div id="CRIGHT" class="btn small yellow cr">C&#9654;</div>
        <div id="CDOWN"  class="btn small yellow cd">C&#9660;</div>
      </div>
    </div>
  </div>
</div>

<script>
  // Bit masks -- keep in sync with packState() in main.cpp.
  const B0 = { A:0x80, B:0x40, Z:0x20, START:0x10, UP:0x08, DOWN:0x04, LEFT:0x02, RIGHT:0x01 };
  const B1 = { L:0x20, R:0x10, CUP:0x08, CDOWN:0x04, CLEFT:0x02, CRIGHT:0x01 };

  const el = {};
  for (const id of [...Object.keys(B0), ...Object.keys(B1)]) el[id] = document.getElementById(id);
  const dot = document.getElementById('dot');
  const sxEl = document.getElementById('sx'), syEl = document.getElementById('sy');
  const conn = document.getElementById('conn');

  function set(id, on) { el[id].classList.toggle('on', !!on); }

  function render(bytes) {
    const b0 = bytes[0], b1 = bytes[1];
    for (const [id, m] of Object.entries(B0)) set(id, b0 & m);
    for (const [id, m] of Object.entries(B1)) set(id, b1 & m);
    // Sign-extend the stick bytes.
    const sx = (bytes[2] << 24) >> 24;
    const sy = (bytes[3] << 24) >> 24;
    sxEl.textContent = sx; syEl.textContent = sy;
    // Map ~[-90,90] to the well radius; screen Y is inverted from N64 Y.
    const R = 38, clamp = v => Math.max(-1, Math.min(1, v / 90));
    dot.style.transform = `translate(${clamp(sx) * R}px, ${-clamp(sy) * R}px)`;
  }

  let ws;
  function connect() {
    ws = new WebSocket(`ws://${location.host}/ws`);
    ws.binaryType = 'arraybuffer';
    ws.onopen    = () => { conn.textContent = 'connected'; conn.className = 'up'; };
    ws.onclose   = () => { conn.textContent = 'disconnected — retrying…'; conn.className = 'down'; setTimeout(connect, 1000); };
    ws.onerror   = () => ws.close();
    ws.onmessage = (e) => render(new Uint8Array(e.data));
  }
  connect();
</script>
</body>
</html>
)HTML";
