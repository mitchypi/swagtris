import init, { GameClient } from "./pkg/tetrisgame2.js";

const COLORS = [
  "transparent",
  "#5dcff5",
  "#3456ff",
  "#ff9f1c",
  "#ffd447",
  "#9be564",
  "#ef476f",
  "#c77dff",
  "#404348",
];

const STORAGE_KEY = "tetris-wasm-settings";
const CONTROLS_KEY = "tetris-wasm-controls";
const VISIBLE_HEIGHT = 20; // render bottom 20 rows
const WIDTH = 10;
let needsResize = true;

let game;
let inputState = {
  left: false,
  right: false,
  soft_drop: false,
  hard_drop: false,
  rotate_ccw: false,
  rotate_cw: false,
  rotate_180: false,
  hold: false,
};
let bindings = loadControls();
let waitingForBind = null;
let previewCount = 6;
let needsResize = true;
let botSocket = null;
let lastBotPiece = null;
let botReady = false;

const actions = [
  { id: "left", label: "Move Left", field: "move_left" },
  { id: "right", label: "Move Right", field: "move_right" },
  { id: "soft_drop", label: "Soft Drop", field: "soft_drop" },
  { id: "hard_drop", label: "Hard Drop", field: "hard_drop" },
  { id: "rotate_ccw", label: "Rotate Left", field: "rotate_left" },
  { id: "rotate_cw", label: "Rotate Right", field: "rotate_right" },
  { id: "rotate_180", label: "Rotate 180", field: "rotate_180" },
  { id: "hold", label: "Hold", field: "hold" },
];

function loadControls() {
  const saved = localStorage.getItem(CONTROLS_KEY);
  return (
    JSON.parse(saved || "null") || {
      left: "ArrowLeft",
      right: "ArrowRight",
      soft_drop: "ArrowDown",
      hard_drop: "Space",
      rotate_ccw: "KeyZ",
      rotate_cw: "ArrowUp",
      rotate_180: "KeyA",
      hold: "KeyC",
    }
  );
}

function persistControls() {
  localStorage.setItem(CONTROLS_KEY, JSON.stringify(bindings));
}

function loadSettings() {
  const saved = localStorage.getItem(STORAGE_KEY);
  return (
    JSON.parse(saved || "null") || {
      das: 133,
      arr: 10,
      softDrop: "Medium",
      gridStyle: "Standard",
      ghost: true,
      pps: 1.8,
      previewCount: 6,
      randomizers: {
        player: { kind: "SevenBag", piece: "I" },
        bot: { kind: "SevenBag", piece: "I" },
      },
    }
  );
}

function persistSettings(settings) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

function setupControlsUI() {
  const container = document.getElementById("controls");
  container.innerHTML = "";
  actions.forEach((action) => {
    const row = document.createElement("div");
    row.className = "control-row";
    const label = document.createElement("span");
    label.textContent = action.label;
    const key = document.createElement("button");
    key.textContent = bindings[action.id] || "Unbound";
    key.addEventListener("click", () => {
      waitingForBind = action.id;
      key.textContent = "Press key";
    });
    row.appendChild(label);
    row.appendChild(key);
    container.appendChild(row);
  });
}

function attachRandomizerSelect(selectId, pieceId) {
  const select = document.getElementById(selectId);
  const piece = document.getElementById(pieceId);
  const update = () => {
    piece.disabled = select.value !== "SinglePiece";
    piece.classList.toggle("muted", piece.disabled);
  };
  select.addEventListener("change", update);
  update();
}

function buildRandomizer(kindSelectId, pieceSelectId) {
  const kind = document.getElementById(kindSelectId).value;
  const piece = document.getElementById(pieceSelectId).value;
  if (kind === "SinglePiece") {
    return { SinglePiece: { piece } };
  }
  return kind;
}

function createGameFromUI() {
  const settings = {
    das: Number(document.getElementById("das").value || 133),
    arr: Number(document.getElementById("arr").value || 10),
    soft_drop: document.getElementById("softDrop").value,
    ghost_enabled: document.getElementById("ghostToggle").value === "true",
    grid: document.getElementById("gridStyle").value,
  };
  const pps = Number(document.getElementById("pps").value || 1.8);
  const randomizers = [
    buildRandomizer("randPlayer", "randPlayerPiece"),
    buildRandomizer("randBot", "randBotPiece"),
  ];
  persistSettings({
    das: settings.das,
    arr: settings.arr,
    softDrop: settings.soft_drop,
    gridStyle: settings.grid,
    ghost: settings.ghost_enabled,
    pps,
    previewCount,
    randomizers: {
      player: { kind: document.getElementById("randPlayer").value, piece: document.getElementById("randPlayerPiece").value },
      bot: { kind: document.getElementById("randBot").value, piece: document.getElementById("randBotPiece").value },
    },
  });
  game = new GameClient(settings, pps, randomizers);
  window.tbpSnapshot = () => game.tbpState(1);
}

function restoreSettings() {
  const saved = loadSettings();
  document.getElementById("das").value = saved.das;
  document.getElementById("arr").value = saved.arr;
  document.getElementById("softDrop").value = saved.softDrop;
  document.getElementById("gridStyle").value = saved.gridStyle;
  document.getElementById("ghostToggle").value = saved.ghost ? "true" : "false";
  document.getElementById("pps").value = saved.pps;
  document.getElementById("ppsValue").textContent = `${saved.pps.toFixed(1)} PPS`;
  document.getElementById("randPlayer").value = saved.randomizers.player.kind;
  document.getElementById("randPlayerPiece").value = saved.randomizers.player.piece;
  document.getElementById("randBot").value = saved.randomizers.bot.kind;
  document.getElementById("randBotPiece").value = saved.randomizers.bot.piece;
  previewCount = saved.previewCount || 6;
  document.getElementById("previewCount").value = previewCount;
  document.getElementById("previewValue").textContent = `${previewCount}`;
}

function bindKeys() {
  window.addEventListener("keydown", (e) => {
    if (waitingForBind) {
      e.preventDefault();
      bindings[waitingForBind] = e.code;
      waitingForBind = null;
      persistControls();
      setupControlsUI();
      return;
    }
    const mapped = Object.entries(bindings).find(([, code]) => code === e.code);
    if (mapped) {
      const [action] = mapped;
      inputState[action] = true;
      e.preventDefault();
    }
  });
  window.addEventListener("keyup", (e) => {
    const mapped = Object.entries(bindings).find(([, code]) => code === e.code);
    if (mapped) {
      const [action] = mapped;
      inputState[action] = false;
      e.preventDefault();
    }
    waitingForBind = null;
    setupControlsUI();
  });
}

function drawBoard(canvas, player, gridStyle) {
  if (needsResize) {
    resizeAllCanvases();
  }
  const ctx = canvas.getContext("2d");
  const w = canvas.width;
  const h = canvas.height;
  const cell = Math.min(w / WIDTH, h / VISIBLE_HEIGHT);
  ctx.clearRect(0, 0, w, h);

  // background grid
  if (gridStyle !== "None") {
    ctx.strokeStyle = "rgba(255,255,255,0.06)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    if (gridStyle === "Vertical" || gridStyle === "Full") {
      for (let x = 0; x <= WIDTH; x++) {
        ctx.moveTo(x * cell, 0);
        ctx.lineTo(x * cell, h);
      }
    }
    if (gridStyle === "Standard" || gridStyle === "Partial" || gridStyle === "Full") {
      for (let y = 0; y <= VISIBLE_HEIGHT; y++) {
        ctx.moveTo(0, y * (h / VISIBLE_HEIGHT));
        ctx.lineTo(w, y * (h / VISIBLE_HEIGHT));
      }
    }
    ctx.stroke();
  }

  // field cells, y=0 is bottom of visible playfield
  for (let y = 0; y < VISIBLE_HEIGHT; y++) {
    for (let x = 0; x < WIDTH; x++) {
      const idx = y * WIDTH + x;
      const colorId = player.field[idx] || 0;
      if (colorId > 0) {
        ctx.fillStyle = COLORS[colorId];
        const drawY = VISIBLE_HEIGHT - 1 - y;
        ctx.fillRect(x * cell, drawY * cell, cell - 1, cell - 1);
      }
    }
  }

  const drawBlocks = (blocks, style) => {
    ctx.fillStyle = style;
    blocks.forEach((p) => {
      const y = VISIBLE_HEIGHT - 1 - p.y;
      if (y >= 0 && y < VISIBLE_HEIGHT) {
        ctx.fillRect(p.x * cell, y * cell, cell - 1, cell - 1);
      }
    });
  };

  if (player.ghost) {
    drawBlocks(player.ghost, "rgba(255,255,255,0.15)");
  }
  const activeColor = COLORS[player.active_color || 7] || "rgba(255,255,255,0.35)";
  drawBlocks(player.active, activeColor);
}

function computeBaseCell() {
  const boardCanvas = document.getElementById("board-player");
  if (boardCanvas) {
    const rect = boardCanvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = rect.width * dpr;
    const h = rect.height * dpr;
    if (w > 0 && h > 0) {
      return Math.min(w / WIDTH, h / VISIBLE_HEIGHT);
    }
  }
  return 20; // fallback
}


function drawHold(canvas, player) {
  if (needsResize) {
    resizeAllCanvases();
  }
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  if (!player.hold || !player.hold_blocks) return;
  const colorId = player.hold_color_id || player.hold;
  const shapes = player.hold_blocks;
  const baseCell = computeBaseCell();
  // Prevent overflow if the hold canvas is smaller.
  const cell = Math.min(baseCell, canvas.width / 5, canvas.height / 5);
  ctx.fillStyle = COLORS[colorId];
  const minX = Math.min(...shapes.map((p) => p.x));
  const maxX = Math.max(...shapes.map((p) => p.x));
  const minY = Math.min(...shapes.map((p) => p.y));
  const maxY = Math.max(...shapes.map((p) => p.y));
  const shapeW = (maxX - minX + 1) * cell;
  const shapeH = (maxY - minY + 1) * cell;
  const originX = (canvas.width - shapeW) / 2 - minX * cell;
  const originY = (canvas.height - shapeH) / 2 + maxY * cell;
  shapes.forEach((p) => {
    const drawX = originX + p.x * cell;
    const drawY = originY - p.y * cell;
    ctx.fillRect(drawX, drawY, cell - 1, cell - 1);
  });
}

function drawNext(canvas, player, count) {
  if (needsResize) {
    resizeAllCanvases();
  }
  const ctx = canvas.getContext("2d");
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const rows = Math.min(count, player.next.length);
  if (rows === 0) return;
  // Fixed 5x5 grid per preview row to keep squares perfectly square.
  const cell = Math.min(computeBaseCell(), canvas.width / 5, canvas.height / (rows * 5 || 1));
  const rowHeight = cell * 5;

  for (let i = 0; i < rows; i++) {
    const colorId = player.next[i];
    const shape = player.next_blocks && player.next_blocks[i];
    if (!colorId || !shape || !shape.length) continue;
    const originX = (canvas.width - cell * 5) / 2;
    const originY = i * rowHeight;
    ctx.fillStyle = COLORS[colorId];
    shape.forEach((p) => {
      const drawY = -p.y;
      ctx.fillRect(
        originX + (p.x + 2) * cell,
        originY + (drawY + 2) * cell,
        cell - 1,
        cell - 1
      );
    });
  }
}

async function main() {
  await init();
  setupControlsUI();
  restoreSettings();
  attachRandomizerSelect("randPlayer", "randPlayerPiece");
  attachRandomizerSelect("randBot", "randBotPiece");
  createGameFromUI();
  bindKeys();
  resizeAllCanvases();
  window.addEventListener("resize", () => (needsResize = true));

  const toggleSettings = document.getElementById("toggleSettings");
  const settingsPanel = document.getElementById("settingsPanel");
  const controlsPanel = document.getElementById("controlsPanel");
  toggleSettings.addEventListener("click", () => {
    settingsPanel.classList.toggle("show");
    controlsPanel.classList.toggle("show");
  });

  document.getElementById("applySettings").addEventListener("click", () => {
    createGameFromUI();
  });
  document.getElementById("pps").addEventListener("input", (e) => {
    document.getElementById("ppsValue").textContent = `${Number(e.target.value).toFixed(1)} PPS`;
  });
  document.getElementById("previewCount").addEventListener("input", (e) => {
    previewCount = Number(e.target.value);
    document.getElementById("previewValue").textContent = `${previewCount}`;
  });

  connectBotBridge();

  let last = performance.now();
  const canvasPlayer = document.getElementById("board-player");
  const canvasBot = document.getElementById("board-bot");
  const holdPlayer = document.getElementById("hold-player");
  const holdBot = document.getElementById("hold-bot");
  const nextPlayer = document.getElementById("next-player");
  const nextBot = document.getElementById("next-bot");

  function loop(ts) {
    const dt = ts - last;
    last = ts;
    if (game) {
      game.setInput(inputState);
      const frame = game.tick(dt);
      const view = frame;
      if (view && view.players) {
        drawBoard(canvasPlayer, view.players[0], view.settings.grid);
        drawBoard(canvasBot, view.players[1], view.settings.grid);
        drawHold(holdPlayer, view.players[0]);
        drawHold(holdBot, view.players[1]);
        drawNext(nextPlayer, view.players[0], previewCount);
        drawNext(nextBot, view.players[1], previewCount);
        driveBot(view.players[1]);
      }
    }
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
}

main();

function resizeAllCanvases() {
  const baseCell = computeBaseCell();
  resizeCanvasesWithBaseCell(baseCell);
  ["board-player", "board-bot", "hold-player", "hold-bot", "next-player", "next-bot"].forEach(
    (id) => {
      const el = document.getElementById(id);
      if (el) {
        syncCanvasSize(el);
      }
    }
  );
  needsResize = false;
}

function syncCanvasSize(canvas) {
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.floor(rect.width * dpr));
  const h = Math.max(1, Math.floor(rect.height * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
}

// Bot bridge (TBP over websocket to cold-clear-2 via bot_bridge)
function connectBotBridge() {
  try {
    botSocket = new WebSocket("ws://127.0.0.1:9000");
  } catch (e) {
    console.warn("Bot bridge connection failed:", e);
    return;
  }
  botSocket.onopen = () => {
    botReady = false;
    sendBot({ type: "rules" });
    lastBotPiece = null;
  };
  botSocket.onmessage = (evt) => {
    try {
      const msg = JSON.parse(evt.data);
      if (msg.type === "ready") {
        botReady = true;
        sendStartState();
      } else if (msg.type === "suggestion" && msg.moves && msg.moves.length > 0) {
        applyBotMove(msg.moves[0]);
      }
    } catch (e) {
      console.warn("Bot bridge parse error", e);
    }
  };
  botSocket.onerror = (e) => console.warn("Bot bridge error", e);
  botSocket.onclose = () => {
    botReady = false;
    botSocket = null;
  };
}

function sendBot(obj) {
  if (botSocket && botSocket.readyState === WebSocket.OPEN) {
    botSocket.send(JSON.stringify(obj));
  }
}

function colorIdToPiece(id) {
  switch (id) {
    case 1:
      return "i";
    case 2:
      return "j";
    case 3:
      return "l";
    case 4:
      return "o";
    case 5:
      return "s";
    case 6:
      return "z";
    case 7:
      return "t";
    default:
      return null;
  }
}

function pieceNameToColor(name) {
  switch ((name || "").toLowerCase()) {
    case "i":
      return 1;
    case "j":
      return 2;
    case "l":
      return 3;
    case "o":
      return 4;
    case "s":
      return 5;
    case "z":
      return 6;
    case "t":
      return 7;
    default:
      return null;
  }
}

function applyBotMove(move) {
  if (!move || !move.location) return;
  const loc = move.location;
  const pieceId = pieceNameToColor(loc.type);
  if (!pieceId) return;
  game.setBotPlan({
    piece: pieceId,
    x: loc.x,
    rotation: (loc.orientation || "").toLowerCase(),
  });
}

let botReady = false;
function driveBot(botPlayer) {
  if (!botReady) return;
  const activeId = botPlayer.active_color;
  if (activeId !== lastBotPiece) {
    lastBotPiece = activeId;
    const pieceName = colorIdToPiece(activeId);
    if (pieceName) {
      sendBot({ type: "new_piece", piece: pieceName });
      sendBot({ type: "suggest" });
    }
  }
}

function sendStartState() {
  const state = game.tbpState(1);
  const board = [];
  const rows = 40;
  const cols = 10;
  for (let y = 0; y < rows; y++) {
    const row = new Array(cols).fill(null);
    board.push(row);
  }
  // state.board is bottom-up visible+buffer (21 rows) flattened bottom to top
  const totalCells = state.board.length;
  const stateRows = Math.floor(totalCells / cols);
  for (let y = 0; y < stateRows; y++) {
    for (let x = 0; x < cols; x++) {
      const idx = y * cols + x;
      const val = state.board[idx];
      if (val && val !== 0) {
        board[y][x] = "x";
      }
    }
  }
  const queue = (state.next || [])
    .map((c) => colorIdToPiece(c))
    .filter(Boolean)
    .slice(0, previewCount);
  const hold = state.hold ? colorIdToPiece(state.hold) : null;
  sendBot({
    type: "start",
    board,
    queue,
    hold,
    combo: 0,
    back_to_back: false,
    randomizer: { type: "seven_bag", bag_state: [] },
  });
}
