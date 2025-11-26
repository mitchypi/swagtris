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
  discard: false,
};
let bindings = loadControls();
let waitingForBind = null;
let previewCount = 6;
const BOT_PLAYER_INDEX = 1;
let botPps = 1.8;
let botSocket = null;
let botReady = false;
let botPendingStart = false;
let awaitingSuggestion = false;
let botLog = [];
let suggestTimer = null;
let gameEnded = false;
let sentStopThisGame = false;
let summaryLogs = [[], []];
let lastStatsSnap = [null, null];

const actions = [
  { id: "left", label: "Move Left", field: "move_left" },
  { id: "right", label: "Move Right", field: "move_right" },
  { id: "soft_drop", label: "Soft Drop", field: "soft_drop" },
  { id: "hard_drop", label: "Hard Drop", field: "hard_drop" },
  { id: "rotate_ccw", label: "Rotate Left", field: "rotate_left" },
  { id: "rotate_cw", label: "Rotate Right", field: "rotate_right" },
  { id: "rotate_180", label: "Rotate 180", field: "rotate_180" },
  { id: "hold", label: "Hold", field: "hold" },
  { id: "discard", label: "Discard", field: "discard" },
  { id: "force_i", label: "Force I Piece", field: "force_i" },
];

function setBotStatus(state, text = "") {
  const dot = document.getElementById("botStatusDot");
  const label = document.getElementById("botStatusText");
  if (!dot || !label) return;
  dot.className = "status-dot";
  label.textContent = text || state;
  switch (state) {
    case "connected":
    case "ready":
      dot.classList.add("ready");
      label.textContent = text || "Connected";
      break;
    case "connecting":
      dot.classList.add("connecting");
      label.textContent = text || "Connecting…";
      break;
    case "error":
      dot.classList.add("error");
      label.textContent = text || "Error";
      break;
    default:
      dot.classList.add("disconnected");
      label.textContent = text || "Disconnected";
  }
}

function logBot(direction, payload) {
  const line = `${direction} ${typeof payload === "string" ? payload : JSON.stringify(payload)}`;
  botLog.push(line);
  console.log(`[bot ${direction}]`, payload);
}

function downloadBotLog() {
  const blob = new Blob([botLog.join("\n")], { type: "text/plain" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `bot-log-${Date.now()}.txt`;
  a.click();
  URL.revokeObjectURL(url);
}

function sendBot(msg) {
  if (!botSocket || botSocket.readyState !== WebSocket.OPEN) return;
  logBot("TX", msg);
  botSocket.send(JSON.stringify(msg));
}

function requestBotSuggestion() {
  if (!botReady || !game || gameEnded || !botSocket || botSocket.readyState !== WebSocket.OPEN || awaitingSuggestion) {
    return;
  }
  if (suggestTimer) {
    clearTimeout(suggestTimer);
    suggestTimer = null;
  }
  const delayMs = 1000 / Math.max(botPps, 0.01);
  suggestTimer = setTimeout(() => {
    suggestTimer = null;
    if (!botReady || !game || !botSocket || botSocket.readyState !== WebSocket.OPEN || awaitingSuggestion) {
      return;
    }
    awaitingSuggestion = true;
    sendBot({ type: "suggest" });
  }, delayMs);
}

function sendBotStart() {
  if (!botReady || !game || gameEnded || !botSocket || botSocket.readyState !== WebSocket.OPEN) {
    botPendingStart = true;
    return;
  }
  try {
    const start = JSON.parse(game.tbpStartJson(BOT_PLAYER_INDEX));
    const payload = {
      type: "start",
      board: start.board,
      queue: start.queue,
      hold: start.hold,
      combo: start.combo,
      back_to_back: start.back_to_back ?? start.backToBack,
    };
    console.debug("[tbp] start", payload);
    sendBot(payload);
    botPendingStart = false;
    awaitingSuggestion = false;
    setBotStatus("connected");
    requestBotSuggestion();
  } catch (err) {
    console.error("Failed to build TBP start:", err);
    setBotStatus("error", "TBP start failed");
  }
}

function handleBotMessage(raw) {
  let msg = null;
  try {
    msg = JSON.parse(raw);
  } catch (err) {
    console.warn("Invalid bot message", raw);
    return;
  }
  switch (msg.type) {
    case "info":
      setBotStatus("connecting", "Negotiating…");
      sendBot({ type: "rules" });
      break;
    case "ready":
      botReady = true;
      setBotStatus("connected");
      sendBotStart();
      break;
    case "suggestion":
      awaitingSuggestion = false;
      applyBotSuggestion(msg);
      break;
    default:
      break;
  }
}

function applyBotSuggestion(msg) {
  if (!msg.moves || !msg.moves.length || !game || gameEnded) return;
  const move = msg.moves[0];
  try {
    const result = game.tbpApplyMove(BOT_PLAYER_INDEX, move);
    if (result.toppedOut) {
      setBotStatus("error", "Bot topped out");
      gameEnded = true;
      sentStopThisGame = true;
      sendBot({ type: "stop" });
      if (suggestTimer) {
        clearTimeout(suggestTimer);
        suggestTimer = null;
      }
      return;
    }
    // Re-issue a full start snapshot for the bot, then ask for the next move.
    sendBotStart();
  } catch (err) {
    console.error("Failed to apply bot move:", err);
    setBotStatus("error", "Apply failed");
  }
}

function connectBot() {
  try {
    botSocket = new WebSocket("ws://127.0.0.1:9000");
  } catch (err) {
    console.error("Failed to open bot socket", err);
    setBotStatus("error", "Socket failed");
    return;
  }
  setBotStatus("connecting");
  botReady = false;
  botPendingStart = true;
  awaitingSuggestion = false;
  botLog = [];
  gameEnded = false;
  sentStopThisGame = false;
  botSocket.addEventListener("open", () => {
    setBotStatus("connecting", "Waiting for info…");
  });
  botSocket.addEventListener("message", (ev) => {
    logBot("RX", ev.data);
    handleBotMessage(ev.data);
  });
  botSocket.addEventListener("close", () => {
    setBotStatus("disconnected");
    botReady = false;
    if (suggestTimer) {
      clearTimeout(suggestTimer);
      suggestTimer = null;
    }
  });
  botSocket.addEventListener("error", () => {
    setBotStatus("error", "Socket error");
    botReady = false;
    if (suggestTimer) {
      clearTimeout(suggestTimer);
      suggestTimer = null;
    }
  });
}

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
      discard: "KeyX",
      force_i: "KeyV",
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
  if (kind === "FiveBag") {
    return "FiveBag";
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
  botPps = pps;
  gameEnded = false;
  sentStopThisGame = false;
  awaitingSuggestion = false;
  setBotStatus(botReady ? "connecting" : "connecting", "Restarting…");
  if (suggestTimer) {
    clearTimeout(suggestTimer);
    suggestTimer = null;
  }
  game = new GameClient(settings, pps, randomizers);
  window.tbpSnapshot = () => game.tbpStart(1);
  botPendingStart = true;
  sendBotStart();
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

function drawBoard(canvas, player, gridStyle, gameOver = false, isWinner = false, pendingGarbage = 0) {
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

  // Incoming garbage bar (Full blocking style).
  if (pendingGarbage > 0) {
    const barW = Math.max(4, cell * 0.35);
    const barX = w - barW - 2;
    const barH = Math.min(h, (pendingGarbage / VISIBLE_HEIGHT) * h);
    ctx.fillStyle = "rgba(239, 71, 111, 0.8)";
    ctx.fillRect(barX, h - barH, barW, barH);
  }

  if (gameOver) {
    ctx.fillStyle = "rgba(0,0,0,0.5)";
    ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = isWinner ? "#5dcf9f" : "#ef476f";
    ctx.font = `bold ${Math.floor(cell * 1.2)}px "Space Grotesk", sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const msg = isWinner ? "Winner" : "Top Out";
    ctx.fillText(msg, w / 2, h / 2);
  }
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

function formatNumber(num, digits = 2) {
  if (!isFinite(num)) return "0.00";
  return num.toFixed(digits);
}

function updateStats(players, dt) {
  for (let i = 0; i < players.length; i++) {
    const player = players[i];
    const prefix = i === 0 ? "player" : "bot";
    const stats = player.stats || {};
    const timeSec = (stats.time_ms || 0) / 1000;
    const timeEl = document.getElementById(`${prefix}-time`);
    const attackEl = document.getElementById(`${prefix}-attack`);
    const finesseEl = document.getElementById(`${prefix}-finesse`);
    const ppsEl = document.getElementById(`${prefix}-pps`);
    const kppEl = document.getElementById(`${prefix}-kpp`);
    const linesEl = document.getElementById(`${prefix}-lines`);
    if (timeEl) timeEl.textContent = `${formatNumber(timeSec, 1)}s`;
    if (attackEl) attackEl.textContent = `${stats.attack ?? 0}`;
    if (finesseEl) finesseEl.textContent = `${stats.finesse ?? 0}`;
    if (ppsEl) ppsEl.textContent = formatNumber(stats.pps || 0, 2);
    if (kppEl) kppEl.textContent = formatNumber(stats.kpp || 0, 2);
    if (linesEl) linesEl.textContent = `${stats.lines_sent ?? 0}`;
  }
}

function renderSummaryLogs() {
  const ids = ["summary-player", "summary-bot"];
  ids.forEach((id, idx) => {
    const el = document.getElementById(id);
    if (!el) return;
    el.innerHTML = "";
    const logs = (window.lastView?.players?.[idx]?.summary) || [];
    for (let i = logs.length - 1; i >= 0; i--) {
      const entry = logs[i];
      const row = document.createElement("div");
      row.className = "entry";
      const time = document.createElement("span");
      time.className = "time";
      time.textContent = `${(entry.time_ms || 0).toFixed(1)}s`;
      const msg = document.createElement("span");
      msg.className = "delta";
      msg.textContent = entry.description || "";
      row.appendChild(time);
      row.appendChild(msg);
      el.appendChild(row);
    }
  });
}

async function main() {
  await init();
  setupControlsUI();
  restoreSettings();
  attachRandomizerSelect("randPlayer", "randPlayerPiece");
  attachRandomizerSelect("randBot", "randBotPiece");
  createGameFromUI();
  connectBot();
  bindKeys();
  window.addEventListener("resize", () => {});

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
  document.getElementById("downloadBotLog").addEventListener("click", downloadBotLog);

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
      // Ensure all fields are present for WASM deserialization.
      const sendState = { force_i: false, ...inputState };
      game.setInput(sendState);
      const frame = game.tick(dt);
      const view = frame;
      if (view && view.players) {
        window.lastView = view;
        const gameOver = view.players.some((p) => p.topped_out);
        const playerWins = gameOver && !view.players[0].topped_out && view.players[1].topped_out;
        const botWins = gameOver && !view.players[1].topped_out && view.players[0].topped_out;
        if (gameOver && !gameEnded) {
          gameEnded = true;
          if (!sentStopThisGame) {
            sendBot({ type: "stop" });
            sentStopThisGame = true;
          }
          if (suggestTimer) {
            clearTimeout(suggestTimer);
            suggestTimer = null;
          }
        } else if (!gameOver) {
          sentStopThisGame = false;
        }
        drawBoard(
          canvasPlayer,
          view.players[0],
          view.settings.grid,
          gameOver,
          playerWins,
          view.players[0].stats?.pending_garbage || 0
        );
        drawBoard(
          canvasBot,
          view.players[1],
          view.settings.grid,
          gameOver,
          botWins,
          view.players[1].stats?.pending_garbage || 0
        );
        drawHold(holdPlayer, view.players[0]);
        drawHold(holdBot, view.players[1]);
        drawNext(nextPlayer, view.players[0], previewCount);
        drawNext(nextBot, view.players[1], previewCount);
        updateStats(view.players, dt);
        renderSummaryLogs();
      }
    }
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
}

main();

window.addEventListener("beforeunload", () => {});
