use rand::seq::SliceRandom;
use rand::thread_rng;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::{from_value, to_value};
use wasm_bindgen::prelude::*;
use web_sys::console;
use tbp::{data as tbp_data, frontend_msg, randomizer as tbp_randomizer, MaybeUnknown};

const WIDTH: usize = 10;
const VISIBLE_HEIGHT: usize = 20; // Jstris-style visible field
const BUFFER_HEIGHT: usize = 20; // single-row, non-colliding buffer
const TOTAL_HEIGHT: usize = VISIBLE_HEIGHT + BUFFER_HEIGHT;
const LOCK_DELAY_MS: f32 = 500.0;

#[wasm_bindgen(start)]
pub fn bootstrap() {
    console_error_panic_hook::set_once();
}

fn log(msg: &str) {
    console::log_1(&JsValue::from_str(msg));
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Tetromino {
    I,
    J,
    L,
    O,
    S,
    Z,
    T,
}

impl Tetromino {
    pub fn all() -> [Tetromino; 7] {
        [
            Tetromino::I,
            Tetromino::J,
            Tetromino::L,
            Tetromino::O,
            Tetromino::S,
            Tetromino::Z,
            Tetromino::T,
        ]
    }

    fn color_id(self) -> u8 {
        match self {
            Tetromino::I => 1,
            Tetromino::J => 2,
            Tetromino::L => 3,
            Tetromino::O => 4,
            Tetromino::S => 5,
            Tetromino::Z => 6,
            Tetromino::T => 7,
        }
    }
}

impl From<tbp_data::Piece> for Tetromino {
    fn from(p: tbp_data::Piece) -> Self {
        match p {
            tbp_data::Piece::I => Tetromino::I,
            tbp_data::Piece::O => Tetromino::O,
            tbp_data::Piece::T => Tetromino::T,
            tbp_data::Piece::L => Tetromino::L,
            tbp_data::Piece::J => Tetromino::J,
            tbp_data::Piece::S => Tetromino::S,
            tbp_data::Piece::Z => Tetromino::Z,
            _ => Tetromino::I,
        }
    }
}

impl From<Tetromino> for tbp_data::Piece {
    fn from(t: Tetromino) -> Self {
        match t {
            Tetromino::I => tbp_data::Piece::I,
            Tetromino::O => tbp_data::Piece::O,
            Tetromino::T => tbp_data::Piece::T,
            Tetromino::L => tbp_data::Piece::L,
            Tetromino::J => tbp_data::Piece::J,
            Tetromino::S => tbp_data::Piece::S,
            Tetromino::Z => tbp_data::Piece::Z,
        }
    }
}

fn from_tbp_orientation(o: tbp_data::Orientation) -> Rotation {
    match o {
        tbp_data::Orientation::North => Rotation::Spawn,
        tbp_data::Orientation::East => Rotation::Right,
        tbp_data::Orientation::South => Rotation::Reverse,
        tbp_data::Orientation::West => Rotation::Left,
        _ => Rotation::Spawn,
    }
}

fn color_to_cell_char(color: u8) -> Option<char> {
    match color {
        1 => Some('I'),
        2 => Some('J'),
        3 => Some('L'),
        4 => Some('O'),
        5 => Some('S'),
        6 => Some('Z'),
        7 => Some('T'),
        8 => Some('G'), // garbage
        _ => None,
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Rotation {
    Spawn = 0,
    Right = 1,
    Reverse = 2,
    Left = 3,
}

impl Rotation {
    fn rotate_cw(self) -> Rotation {
        match self {
            Rotation::Spawn => Rotation::Right,
            Rotation::Right => Rotation::Reverse,
            Rotation::Reverse => Rotation::Left,
            Rotation::Left => Rotation::Spawn,
        }
    }

    fn rotate_ccw(self) -> Rotation {
        match self {
            Rotation::Spawn => Rotation::Left,
            Rotation::Left => Rotation::Reverse,
            Rotation::Reverse => Rotation::Right,
            Rotation::Right => Rotation::Spawn,
        }
    }

    fn rotate_180(self) -> Rotation {
        match self {
            Rotation::Spawn => Rotation::Reverse,
            Rotation::Reverse => Rotation::Spawn,
            Rotation::Right => Rotation::Left,
            Rotation::Left => Rotation::Right,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i8,
    pub y: i8,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GameSettings {
    pub das: u32,
    pub arr: u32,
    pub soft_drop: SoftDropSpeed,
    pub ghost_enabled: bool,
    pub grid: GridStyle,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            das: 133,
            arr: 10,
            soft_drop: SoftDropSpeed::Medium,
            ghost_enabled: true,
            grid: GridStyle::Standard,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum SoftDropSpeed {
    Slow,
    Medium,
    Fast,
    Ultra,
    Instant,
}

impl SoftDropSpeed {
    fn factor(self) -> f32 {
        match self {
            SoftDropSpeed::Slow => 1.2,
            SoftDropSpeed::Medium => 2.0,
            SoftDropSpeed::Fast => 5.0,
            SoftDropSpeed::Ultra => 20.0,
            SoftDropSpeed::Instant => 999.0,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum GridStyle {
    None,
    Standard,
    Partial,
    Vertical,
    Full,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum RandomizerKind {
    TrueRandom,
    SevenBag,
    SinglePiece { piece: Tetromino },
    LoveTris,
}

impl Default for RandomizerKind {
    fn default() -> Self {
        RandomizerKind::SevenBag
    }
}

trait Randomizer: std::any::Any {
    fn next(&mut self, board: &Board) -> Tetromino;
    fn bag_state(&self) -> Option<Vec<Tetromino>> {
        None
    }
}

struct TrueRandom;

impl Randomizer for TrueRandom {
    fn next(&mut self, _board: &Board) -> Tetromino {
        let mut rng = thread_rng();
        *Tetromino::all().choose(&mut rng).unwrap()
    }
}

struct SinglePiece {
    piece: Tetromino,
}

impl Randomizer for SinglePiece {
    fn next(&mut self, _board: &Board) -> Tetromino {
        self.piece
    }
}

struct SevenBag {
    bag: Vec<Tetromino>,
}

impl SevenBag {
    fn new() -> Self {
        Self { bag: Vec::new() }
    }

    fn refill(&mut self) {
        self.bag = Tetromino::all().to_vec();
        self.bag.shuffle(&mut thread_rng());
    }
}

impl Randomizer for SevenBag {
    fn next(&mut self, _board: &Board) -> Tetromino {
        if self.bag.is_empty() {
            self.refill();
        }
        self.bag.pop().unwrap()
    }

    fn bag_state(&self) -> Option<Vec<Tetromino>> {
        Some(self.bag.clone())
    }
}

struct LoveTris {
    bag: SevenBag,
}

impl LoveTris {
    fn new() -> Self {
        Self {
            bag: SevenBag::new(),
        }
    }

    fn score_candidate(board: &Board, piece: Tetromino) -> i32 {
        let mut best = i32::MIN;
        for rot in [
            Rotation::Spawn,
            Rotation::Right,
            Rotation::Reverse,
            Rotation::Left,
        ] {
            let shape = shape_blocks(piece, rot);
            for x in -2..WIDTH as i32 + 2 {
                if let Some(h) = board.lowest_drop_height(x, &shape) {
                    let mut simulated = board.clone();
                    simulated.lock_piece(x, h, &shape, piece.color_id());
                    let lines = simulated.clear_lines();
                    let holes = simulated.hole_count();
                    let height_penalty = simulated.max_height() as i32 * 2;
                    let score = (lines as i32 * 40) - (holes as i32 * 8) - height_penalty;
                    if score > best {
                        best = score;
                    }
                }
            }
        }
        best
    }
}

impl Randomizer for LoveTris {
    fn next(&mut self, board: &Board) -> Tetromino {
        if self.bag.bag.is_empty() {
            self.bag.refill();
        }
        let mut best_index = 0;
        let mut best_score = i32::MIN;
        for (idx, piece) in self.bag.bag.iter().enumerate() {
            let score = Self::score_candidate(board, *piece);
            if score > best_score {
                best_index = idx;
                best_score = score;
            }
        }
        self.bag.bag.remove(best_index)
    }

    fn bag_state(&self) -> Option<Vec<Tetromino>> {
        self.bag.bag_state()
    }
}

fn randomizer_from_kind(kind: RandomizerKind) -> Box<dyn Randomizer> {
    match kind {
        RandomizerKind::TrueRandom => Box::new(TrueRandom),
        RandomizerKind::SevenBag => Box::new(SevenBag::new()),
        RandomizerKind::SinglePiece { piece } => Box::new(SinglePiece { piece }),
        RandomizerKind::LoveTris => Box::new(LoveTris::new()),
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub soft_drop: bool,
    pub hard_drop: bool,
    pub rotate_ccw: bool,
    pub rotate_cw: bool,
    pub rotate_180: bool,
    pub hold: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            left: false,
            right: false,
            soft_drop: false,
            hard_drop: false,
            rotate_ccw: false,
            rotate_cw: false,
            rotate_180: false,
            hold: false,
        }
    }
}

impl From<InputFrame> for InputState {
    fn from(value: InputFrame) -> Self {
        Self {
            left: value.left,
            right: value.right,
            soft_drop: value.soft_drop,
            hard_drop: value.hard_drop,
            rotate_ccw: value.rotate_ccw,
            rotate_cw: value.rotate_cw,
            rotate_180: value.rotate_180,
            hold: value.hold,
        }
    }
}

fn rotate_point(p: Point, rot: Rotation) -> Point {
    match rot {
        Rotation::Spawn => p,
        Rotation::Right => Point { x: p.y, y: -p.x },
        Rotation::Reverse => Point { x: -p.x, y: -p.y },
        Rotation::Left => Point { x: -p.y, y: p.x },
    }
}

fn shape_blocks(piece: Tetromino, rotation: Rotation) -> [Point; 4] {
    // Guideline SRS shapes with correct rotation centers:
    // I/O rotate about grid intersections; JLSTZ rotate about a mino center.
    match piece {
        Tetromino::I => match rotation {
            Rotation::Spawn => [
                Point { x: -1, y: 0 },
                Point { x: 0, y: 0 },
                Point { x: 1, y: 0 },
                Point { x: 2, y: 0 },
            ],
            Rotation::Right => [
                Point { x: 1, y: 1 },
                Point { x: 1, y: 0 },
                Point { x: 1, y: -1 },
                Point { x: 1, y: -2 },
            ],
            Rotation::Reverse => [
                Point { x: -1, y: -1 },
                Point { x: 0, y: -1 },
                Point { x: 1, y: -1 },
                Point { x: 2, y: -1 },
            ],
            Rotation::Left => [
                Point { x: 0, y: 1 },
                Point { x: 0, y: 0 },
                Point { x: 0, y: -1 },
                Point { x: 0, y: -2 },
            ],
        },
        Tetromino::O => match rotation {
            Rotation::Spawn => [
                Point { x: 0, y: 0 },
                Point { x: 1, y: 0 },
                Point { x: 0, y: 1 },
                Point { x: 1, y: 1 },
            ],
            Rotation::Right => [
                Point { x: 1, y: 0 },
                Point { x: 1, y: 1 },
                Point { x: 2, y: 0 },
                Point { x: 2, y: 1 },
            ],
            Rotation::Reverse => [
                Point { x: 1, y: -1 },
                Point { x: 2, y: -1 },
                Point { x: 1, y: 0 },
                Point { x: 2, y: 0 },
            ],
            Rotation::Left => [
                Point { x: 0, y: -1 },
                Point { x: 1, y: -1 },
                Point { x: 0, y: 0 },
                Point { x: 1, y: 0 },
            ],
        },
        _ => {
            // Base spawn shapes per Guideline SRS (y up).
            let base = match piece {
                Tetromino::T => [
                    Point { x: -1, y: 0 },
                    Point { x: 0, y: 0 },
                    Point { x: 1, y: 0 },
                    Point { x: 0, y: 1 },
                ],
                Tetromino::J => [
                    Point { x: -1, y: 0 },
                    Point { x: 0, y: 0 },
                    Point { x: 1, y: 0 },
                    Point { x: -1, y: 1 },
                ],
                Tetromino::L => [
                    Point { x: -1, y: 0 },
                    Point { x: 0, y: 0 },
                    Point { x: 1, y: 0 },
                    Point { x: 1, y: 1 },
                ],
                Tetromino::S => [
                    Point { x: -1, y: 0 },
                    Point { x: 0, y: 0 },
                    Point { x: 0, y: 1 },
                    Point { x: 1, y: 1 },
                ],
                Tetromino::Z => [
                    Point { x: -1, y: 1 },
                    Point { x: 0, y: 1 },
                    Point { x: 0, y: 0 },
                    Point { x: 1, y: 0 },
                ],
                Tetromino::I | Tetromino::O => unreachable!(),
            };
            let mut rotated = [Point { x: 0, y: 0 }; 4];
            for (i, p) in base.iter().enumerate() {
                rotated[i] = rotate_point(*p, rotation);
            }
            rotated
        }
    }
}

fn tbp_anchor_offset(piece: Tetromino, rotation: Rotation) -> Point {
    match piece {
        Tetromino::I => match rotation {
            Rotation::Spawn => Point { x: 0, y: 0 },  // middle-left mino
            Rotation::Right => Point { x: 1, y: 1 },  // middle-top mino
            Rotation::Reverse => Point { x: 1, y: -1 }, // middle-right mino
            Rotation::Left => Point { x: 0, y: -1 },   // middle-bottom mino
        },
        Tetromino::O => match rotation {
            Rotation::Spawn => Point { x: 0, y: 0 },   // bottom-left mino
            Rotation::Right => Point { x: 1, y: 1 },   // top-left mino
            Rotation::Reverse => Point { x: 2, y: 0 }, // top-right mino
            Rotation::Left => Point { x: 1, y: -1 },   // bottom-right mino
        },
        _ => Point { x: 0, y: 0 }, // JLTSZ centers align with the rotation origin
    }
}

fn spawn_blocks(piece: Tetromino) -> [Point; 4] {
    shape_blocks(piece, Rotation::Spawn)
}

#[derive(Clone)]
struct ActivePiece {
    piece: Tetromino,
    rotation: Rotation,
    x: i32,
    y: i32,
    lock_timer: f32,
    move_resets: u8,
}

impl ActivePiece {
    fn new(piece: Tetromino) -> Self {
        Self {
            piece,
            rotation: Rotation::Spawn,
            x: 4,
            // Spawn so the lowest cells are visible; buffer row above is non-colliding.
            y: (VISIBLE_HEIGHT as i32) - 1,
            lock_timer: LOCK_DELAY_MS,
            move_resets: 15,
        }
    }

    fn blocks(&self) -> [Point; 4] {
        shape_blocks(self.piece, self.rotation)
    }
}

#[derive(Clone)]
struct Board {
    cells: [[u8; WIDTH]; TOTAL_HEIGHT],
}

impl Board {
    fn new() -> Self {
        Self {
            cells: [[0; WIDTH]; TOTAL_HEIGHT],
        }
    }

    fn is_occupied(&self, x: i32, y: i32) -> bool {
        if x < 0 || x >= WIDTH as i32 {
            return true;
        }
        if y < 0 {
            return true;
        }
        if y >= TOTAL_HEIGHT as i32 {
            return true;
        }
        // Buffer rows are non-colliding.
        if y >= VISIBLE_HEIGHT as i32 {
            return false;
        }
        self.cells[y as usize][x as usize] != 0
    }

    fn collision(&self, ap: &ActivePiece) -> bool {
        for b in ap.blocks() {
            let x = ap.x + b.x as i32;
            let y = ap.y + b.y as i32;
            if self.is_occupied(x, y) {
                return true;
            }
        }
        false
    }

    fn lock_piece(&mut self, x: i32, y: i32, blocks: &[Point; 4], color: u8) {
        for b in blocks {
            let px = x + b.x as i32;
            let py = y + b.y as i32;
            if px >= 0 && px < WIDTH as i32 && py >= 0 && py < TOTAL_HEIGHT as i32 {
                self.cells[py as usize][px as usize] = color;
            }
        }
    }

    fn clear_lines(&mut self) -> usize {
        let mut cleared = 0;
        let mut y = 0;
        while y < VISIBLE_HEIGHT {
            if self.cells[y].iter().all(|&c| c != 0) {
                cleared += 1;
                // move everything above this line down by one
                for pull in (y + 1)..TOTAL_HEIGHT {
                    self.cells[pull - 1] = self.cells[pull];
                }
                self.cells[TOTAL_HEIGHT - 1] = [0; WIDTH]; // top becomes empty (buffer row cleared too)
                // do not increment y to recheck the same row after pull-down
            } else {
                y += 1;
            }
        }
        cleared
    }

    fn hole_count(&self) -> usize {
        let mut holes = 0;
        for x in 0..WIDTH {
            let mut found = false;
            for y in (0..TOTAL_HEIGHT).rev() {
                if self.cells[y][x] != 0 {
                    found = true;
                } else if found {
                    holes += 1;
                }
            }
        }
        holes
    }

    fn max_height(&self) -> usize {
        for y in (0..TOTAL_HEIGHT).rev() {
            if self.cells[y].iter().any(|&c| c != 0) {
                return y + 1;
            }
        }
        0
    }

    fn visible_empty(&self) -> bool {
        for y in 0..VISIBLE_HEIGHT {
            if self.cells[y].iter().any(|&c| c != 0) {
                return false;
            }
        }
        true
    }

    fn lowest_drop_height(&self, x: i32, blocks: &[Point; 4]) -> Option<i32> {
        let mut y = TOTAL_HEIGHT as i32 - 1;
        while y >= 0 {
            if blocks.iter().all(|b| {
                let px = x + b.x as i32;
                let py = y + b.y as i32;
                px >= 0 && px < WIDTH as i32 && py >= 0 && py < TOTAL_HEIGHT as i32
            }) && !blocks.iter().any(|b| {
                let px = x + b.x as i32;
                let py = y + b.y as i32;
                // allow in buffer
                self.is_occupied(px, py)
            }) {
                return Some(y);
            }
            y -= 1;
        }
        None
    }

    fn add_garbage(&mut self, lines: u32) -> bool {
        if lines == 0 {
            return false;
        }
        let mut rng = thread_rng();
        let hole = rng.gen_range(0..WIDTH);
        for _ in 0..lines {
            for y in (1..TOTAL_HEIGHT).rev() {
                self.cells[y] = self.cells[y - 1];
            }
            let mut row = [8u8; WIDTH];
            row[hole] = 0;
            self.cells[0] = row;
        }
        self.max_height() > VISIBLE_HEIGHT
    }
}

#[derive(Default)]
struct KickTable;

impl KickTable {
    fn kicks(piece: Tetromino, from: Rotation, to: Rotation) -> Vec<(i32, i32)> {
        let idx = match (from, to) {
            (Rotation::Spawn, Rotation::Right) => 0,
            (Rotation::Right, Rotation::Spawn) => 1,
            (Rotation::Right, Rotation::Reverse) => 2,
            (Rotation::Reverse, Rotation::Right) => 3,
            (Rotation::Reverse, Rotation::Left) => 4,
            (Rotation::Left, Rotation::Reverse) => 5,
            (Rotation::Left, Rotation::Spawn) => 6,
            (Rotation::Spawn, Rotation::Left) => 7,
            _ => 0,
        };
        // From Guideline SRS tables (JLSTZ) and I, O.
        const JLSTZ: [[(i32, i32); 5]; 8] = [
            [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 0->R
            [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],    // R->0
            [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],    // R->2
            [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],// 2->R
            [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],   // 2->L
            [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)], // L->2
            [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)], // L->0
            [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],   // 0->L
        ];
        const I: [[(i32, i32); 5]; 8] = [
            [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)], // 0->R
            [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)], // R->0
            [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)], // R->2
            [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)], // 2->R
            [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)], // 2->L
            [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)], // L->2
            [(0, 0), (1, 0), (2, 0), (1, -2), (2, -1)],  // L->0
            [(0, 0), (-1, 0), (-2, 0), (-1, 2), (-2, 1)],// 0->L
        ];
        match piece {
            Tetromino::I => I[idx].to_vec(),
            Tetromino::O => vec![(0, 0)],
            _ => JLSTZ[idx].to_vec(),
        }
    }
}

#[derive(Serialize)]
pub struct PlayerStats {
    pub time_ms: f32,
    pub pieces: u32,
    pub keys: u32,
    pub attack: u32,
    pub finesse: u32,
    pub lines_sent: u32,
}

impl Default for PlayerStats {
    fn default() -> Self {
        Self {
            time_ms: 0.0,
            pieces: 0,
            keys: 0,
            attack: 0,
            finesse: 0,
            lines_sent: 0,
        }
    }
}

#[derive(Serialize)]
pub struct PlayerStatsView {
    pub time_ms: f32,
    pub pieces: u32,
    pub keys: u32,
    pub attack: u32,
    pub finesse: u32,
    pub pps: f32,
    pub kpp: f32,
    pub lines_sent: u32,
    pub pending_garbage: u32,
}

#[derive(Serialize)]
pub struct PlayerView {
    pub field: Vec<u8>,
    pub active: Vec<Point>,
    pub active_color: u8,
    pub active_piece: u8,
    pub active_rotation: String,
    pub ghost: Vec<Point>,
    pub hold: Option<u8>,
    pub hold_blocks: Option<Vec<Point>>,
    pub hold_color_id: Option<u8>,
    pub next: Vec<u8>,
    pub next_blocks: Vec<Vec<Point>>,
    pub topped_out: bool,
    pub stats: PlayerStatsView,
}

#[derive(Serialize)]
pub struct FrameView {
    pub players: Vec<PlayerView>,
    pub settings: GameSettings,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedMoveResult {
    pub lines_cleared: usize,
    pub topped_out: bool,
    pub active_piece: Option<tbp_data::Piece>,
    pub new_queue_piece: Option<tbp_data::Piece>,
    pub combo: u32,
    pub back_to_back: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ControlBindings {
    pub move_left: String,
    pub move_right: String,
    pub soft_drop: String,
    pub hard_drop: String,
    pub rotate_ccw: String,
    pub rotate_cw: String,
    pub rotate_180: String,
    pub hold: String,
}

impl Default for ControlBindings {
    fn default() -> Self {
        Self {
            move_left: "ArrowLeft".to_string(),
            move_right: "ArrowRight".to_string(),
            soft_drop: "ArrowDown".to_string(),
            hard_drop: "Space".to_string(),
            rotate_ccw: "KeyZ".to_string(),
            rotate_cw: "ArrowUp".to_string(),
            rotate_180: "KeyA".to_string(),
            hold: "KeyC".to_string(),
        }
    }
}

struct Player {
    board: Board,
    active: ActivePiece,
    queue: Vec<Tetromino>,
    hold: Option<Tetromino>,
    held_on_turn: bool,
    last_action_was_t_spin: bool,
    randomizer: Box<dyn Randomizer>,
    randomizer_kind: RandomizerKind,
    topped_out: bool,
    pending_garbage: u32,
    combo: u32,
    back_to_back: bool,
    last_refill_added: Option<Tetromino>,
}

impl Player {
    fn new(randomizer_kind: RandomizerKind) -> Self {
        let mut randomizer = randomizer_from_kind(randomizer_kind.clone());
        let mut queue = Vec::new();
        for _ in 0..6 {
            queue.push(randomizer.next(&Board::new()));
        }
        let first = queue.remove(0);
        Self {
            board: Board::new(),
            active: ActivePiece::new(first),
            queue,
            hold: None,
            held_on_turn: false,
            last_action_was_t_spin: false,
            randomizer,
            randomizer_kind,
            topped_out: false,
            pending_garbage: 0,
            combo: 0,
            back_to_back: false,
            last_refill_added: None,
        }
    }

    fn set_randomizer(&mut self, kind: RandomizerKind) {
        self.randomizer_kind = kind.clone();
        self.randomizer = randomizer_from_kind(kind);
        self.queue.clear();
        self.refill_queue();
        self.hold = None;
        self.spawn_next();
    }

    fn refill_queue(&mut self) {
        self.last_refill_added = None;
        while self.queue.len() < 6 {
            let piece = self.randomizer.next(&self.board);
            self.queue.push(piece);
            self.last_refill_added = Some(piece);
        }
    }

    fn spawn_next(&mut self) {
        self.held_on_turn = false;
        self.last_action_was_t_spin = false;
        let next_piece = self.queue.remove(0);
        self.refill_queue();
        self.active = ActivePiece::new(next_piece);
        if self.board.collision(&self.active) {
            self.topped_out = true;
            log("Top out on spawn");
        }
    }

    fn hard_drop(&mut self) -> (usize, bool) {
        let mut landing_y = self.active.y;
        loop {
            let test = ActivePiece {
                y: landing_y - 1,
                ..self.active.clone()
            };
            if self.board.collision(&test) {
                break;
            } else {
                landing_y -= 1;
            }
            if landing_y < 0 {
                break;
            }
        }
        self.active.y = landing_y;
        self.lock_piece()
    }

    fn lock_piece(&mut self) -> (usize, bool) {
        let color = self.active.piece.color_id();
        let blocks = self.active.blocks();
        self.board
            .lock_piece(self.active.x, self.active.y, &blocks, color);
        let cleared = self.board.clear_lines();
        let was_t_spin = self.last_action_was_t_spin && self.active.piece == Tetromino::T && cleared > 0;
        self.spawn_next();
        (cleared, was_t_spin)
    }
}

impl Versus {
    fn on_piece_locked(&mut self, idx: usize, cleared: usize, is_t_spin: bool) {
        // Work with locals to avoid aliasing self borrows.
        let attack_out: u32;
        let mut apply_garbage = false;
        {
            let player = &mut self.players[idx];
            let stats = &mut self.stats[idx];
            stats.pieces = stats.pieces.saturating_add(1);

            if cleared > 0 {
                player.combo = player.combo.saturating_add(1);
            } else {
                player.combo = 0;
                apply_garbage = true;
            }

            let perfect_clear = player.board.visible_empty();
            let mut attack = if is_t_spin && cleared > 0 {
                match cleared {
                    1 => self.attack_table.t_spin_single as u32,
                    2 => self.attack_table.t_spin_double as u32,
                    _ => self.attack_table.t_spin_triple as u32,
                }
            } else {
                match cleared {
                    0 => self.attack_table._0_lines as u32,
                    1 => self.attack_table._1_line_single as u32,
                    2 => self.attack_table._2_lines_double as u32,
                    3 => self.attack_table._3_lines_triple as u32,
                    _ => self.attack_table._4_lines as u32,
                }
            };

            let combo_idx = player.combo.saturating_sub(1);
            let combo_bonus = match combo_idx {
                0 => self.combo_table.c0,
                1 => self.combo_table.c1,
                2 => self.combo_table.c2,
                3 => self.combo_table.c3,
                4 => self.combo_table.c4,
                5 => self.combo_table.c5,
                6 => self.combo_table.c6,
                7 => self.combo_table.c7,
                8 => self.combo_table.c8,
                9 => self.combo_table.c9,
                10 => self.combo_table.c10,
                11 => self.combo_table.c11,
                _ => self.combo_table.c12_plus,
            } as u32;
            attack = attack.saturating_add(combo_bonus);

            if player.back_to_back && cleared >= 4 {
                attack = attack.saturating_add(self.attack_table.back_to_back_bonus as u32);
            }
            if perfect_clear {
                attack = attack.saturating_add(self.attack_table.perfect_clear as u32);
            }
            let attack_before_cancel = attack;
            player.back_to_back = cleared >= 4;

            if attack > 0 {
                let pending = &mut player.pending_garbage;
                if *pending >= attack {
                    *pending -= attack;
                    attack = 0;
                } else {
                    attack -= *pending;
                    *pending = 0;
                }
            }

            attack_out = attack;
            stats.attack = stats.attack.saturating_add(attack_before_cancel);
        }

        // Apply any blocked garbage now that combo is broken.
        if apply_garbage {
            let pending = self.players[idx].pending_garbage;
            if pending > 0 {
                let overflow = self.players[idx].board.add_garbage(pending);
                if overflow {
                    self.players[idx].topped_out = true;
                }
                self.players[idx].pending_garbage = 0;
            }
        }

        // Deliver outgoing attack after previous borrows are released.
        if attack_out > 0 {
            let opp = if idx == 0 { 1 } else { 0 };
            self.players[opp].pending_garbage =
                self.players[opp].pending_garbage.saturating_add(attack_out);
            self.stats[idx].lines_sent = self.stats[idx].lines_sent.saturating_add(attack_out);
        }
    }

}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct InputFrame {
    pub left: bool,
    pub right: bool,
    pub soft_drop: bool,
    pub hard_drop: bool,
    pub rotate_ccw: bool,
    pub rotate_cw: bool,
    pub rotate_180: bool,
    pub hold: bool,
}

impl Default for InputFrame {
    fn default() -> Self {
        InputFrame {
            left: false,
            right: false,
            soft_drop: false,
            hard_drop: false,
            rotate_ccw: false,
            rotate_cw: false,
            rotate_180: false,
            hold: false,
        }
    }
}

impl From<InputState> for InputFrame {
    fn from(value: InputState) -> Self {
        Self {
            left: value.left,
            right: value.right,
            soft_drop: value.soft_drop,
            hard_drop: value.hard_drop,
            rotate_ccw: value.rotate_ccw,
            rotate_cw: value.rotate_cw,
            rotate_180: value.rotate_180,
            hold: value.hold,
        }
    }
}

fn count_input_edges(prev: &InputState, curr: &InputState) -> u32 {
    let mut edges = 0;
    let fields = [
        (prev.left, curr.left),
        (prev.right, curr.right),
        (prev.soft_drop, curr.soft_drop),
        (prev.hard_drop, curr.hard_drop),
        (prev.rotate_ccw, curr.rotate_ccw),
        (prev.rotate_cw, curr.rotate_cw),
        (prev.rotate_180, curr.rotate_180),
        (prev.hold, curr.hold),
    ];
    for (p, c) in fields {
        if !p && c {
            edges += 1;
        }
    }
    edges
}

struct Controller {
    inputs: InputState,
    last_hard_drop: bool,
    last_dir: i32,
    das_timer: f32,
    arr_timer: f32,
    shifted_initial: bool,
    last_rotate_cw: bool,
    last_rotate_ccw: bool,
    last_rotate_180: bool,
}

impl Controller {
    fn new() -> Self {
        Self {
            inputs: InputState::default(),
            last_hard_drop: false,
            last_dir: 0,
            das_timer: 0.0,
            arr_timer: 0.0,
            shifted_initial: false,
            last_rotate_cw: false,
            last_rotate_ccw: false,
            last_rotate_180: false,
        }
    }

    fn update_inputs(&mut self, incoming: InputFrame) {
        self.inputs.left = incoming.left;
        self.inputs.right = incoming.right;
        self.inputs.soft_drop = incoming.soft_drop;
        self.inputs.hard_drop = incoming.hard_drop;
        self.inputs.rotate_ccw = incoming.rotate_ccw;
        self.inputs.rotate_cw = incoming.rotate_cw;
        self.inputs.rotate_180 = incoming.rotate_180;
        self.inputs.hold = incoming.hold;
    }

    fn take_hard_drop(&mut self) -> bool {
        let fire = self.inputs.hard_drop && !self.last_hard_drop;
        self.last_hard_drop = self.inputs.hard_drop;
        fire
    }

    fn take_rotate_cw(&mut self) -> bool {
        let fire = self.inputs.rotate_cw && !self.last_rotate_cw;
        self.last_rotate_cw = self.inputs.rotate_cw;
        fire
    }

    fn take_rotate_ccw(&mut self) -> bool {
        let fire = self.inputs.rotate_ccw && !self.last_rotate_ccw;
        self.last_rotate_ccw = self.inputs.rotate_ccw;
        fire
    }

    fn take_rotate_180(&mut self) -> bool {
        let fire = self.inputs.rotate_180 && !self.last_rotate_180;
        self.last_rotate_180 = self.inputs.rotate_180;
        fire
    }
}

struct BotConfig {
    pps: f32,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self { pps: 1.8 }
    }
}

struct BotDriver {
    config: BotConfig,
    think_timer: f32,
}

impl BotDriver {
    fn new(config: BotConfig) -> Self {
        Self {
            config,
            think_timer: 0.0,
        }
    }

    fn update(&mut self, player: &mut Player, dt_ms: f32) -> InputFrame {
        let mut frame = InputFrame {
            left: false,
            right: false,
            soft_drop: false,
            hard_drop: false,
            rotate_ccw: false,
            rotate_cw: false,
            rotate_180: false,
            hold: false,
        };
        self.think_timer += dt_ms;
        let piece_time = 1000.0 / self.config.pps.max(0.1);
        if self.think_timer >= piece_time {
            self.think_timer = 0.0;
            let best = find_safe_column(&player.board, player.active.piece);
            if let Some(plan) = best {
                frame = plan;
            } else {
                frame.hard_drop = true;
            }
        }
        frame
    }
}

fn find_safe_column(board: &Board, piece: Tetromino) -> Option<InputFrame> {
    let mut rng = thread_rng();
    let mut columns: Vec<i32> = (0..WIDTH as i32).collect();
    columns.shuffle(&mut rng);

    let mut best_col: Option<i32> = None;
    let mut best_height = usize::MAX;
    for col in columns {
        let height = (0..TOTAL_HEIGHT)
            .rev()
            .find(|&y| board.cells[y][col as usize] != 0)
            .map(|y| y + 1)
            .unwrap_or(0);
        if height < best_height {
            best_height = height;
            best_col = Some(col);
        }
    }

    if let Some(col) = best_col {
        let mut frame = InputFrame {
            left: false,
            right: false,
            soft_drop: false,
            hard_drop: true,
            rotate_ccw: false,
            rotate_cw: false,
            rotate_180: false,
            hold: false,
        };
        if col < 4 {
            frame.left = true;
        } else if col > 4 {
            frame.right = true;
        }
        if piece == Tetromino::I && best_height + 4 > VISIBLE_HEIGHT + BUFFER_HEIGHT - 2 {
            frame.rotate_cw = true;
        }
        return Some(frame);
    }
    None
}

struct Versus {
    players: [Player; 2],
    controllers: [Controller; 2],
    settings: GameSettings,
    bot_driver: BotDriver,
    use_internal_bot: bool,
    fall_accum: [f32; 2],
    gravity_ms: f32,
    stats: [PlayerStats; 2],
    last_inputs: [InputState; 2],
    attack_table: AttackTable,
    combo_table: ComboTable,
}

impl Versus {
    fn new(settings: GameSettings, bot_config: BotConfig, randomizers: [RandomizerKind; 2]) -> Self {
        Self {
            players: [
                Player::new(randomizers[0].clone()),
                Player::new(randomizers[1].clone()),
            ],
            controllers: [Controller::new(), Controller::new()],
            settings,
            bot_driver: BotDriver::new(bot_config),
            use_internal_bot: false, // external bot is expected by default; can be toggled on if desired
            fall_accum: [0.0, 0.0],
            gravity_ms: 1000.0,
            stats: [PlayerStats::default(), PlayerStats::default()],
            last_inputs: [InputState::default(), InputState::default()],
            attack_table: default_attack_table(),
            combo_table: default_combo_table(),
        }
    }

    fn tick(&mut self, dt_ms: f32, input0: InputFrame) {
        if self.players[0].topped_out || self.players[1].topped_out {
            return;
        }
        for s in self.stats.iter_mut() {
            s.time_ms += dt_ms;
        }
        self.controllers[0].update_inputs(input0);
        self.stats[0].keys += count_input_edges(&self.last_inputs[0], &input0.clone().into());
        self.last_inputs[0] = input0.into();
        if self.use_internal_bot {
            let bot_input = self.bot_driver.update(&mut self.players[1], dt_ms);
            self.controllers[1].update_inputs(bot_input);
            self.stats[1].keys +=
                count_input_edges(&self.last_inputs[1], &bot_input.clone().into());
            self.last_inputs[1] = bot_input.into();
        } else {
            let idle = InputFrame::default();
            self.controllers[1].update_inputs(idle);
        }

        for idx in 0..2 {
            if idx == 1 && !self.use_internal_bot {
                continue;
            }
            let is_bot = idx == 1;
            let inputs = self.controllers[idx].inputs.clone();
            self.advance_player(idx, dt_ms, inputs, is_bot);
        }
    }

    fn advance_player(&mut self, idx: usize, dt_ms: f32, inputs: InputState, _is_bot: bool) {
        if self.players[idx].topped_out {
            return;
        }
        let (mut moved, mut rotated) = (false, false);
        if self.controllers[idx].take_hard_drop() {
            let (cleared, t_spin) = self.players[idx].hard_drop();
            self.on_piece_locked(idx, cleared, t_spin);
            self.fall_accum[idx] = 0.0;
            return;
        }
        if self.controllers[idx].take_rotate_cw() {
            rotated |= self.try_rotate(idx, true, false);
        }
        if self.controllers[idx].take_rotate_ccw() {
            rotated |= self.try_rotate(idx, false, false);
        }
        if self.controllers[idx].take_rotate_180() {
            rotated |= self.try_rotate(idx, true, true);
        }
        let dir = match (inputs.left, inputs.right) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        };
        {
            let ctrl = &mut self.controllers[idx];
            if dir != ctrl.last_dir {
                ctrl.das_timer = 0.0;
                ctrl.arr_timer = 0.0;
                ctrl.shifted_initial = false;
                ctrl.last_dir = dir;
            }
        }
        let mut das_timer = self.controllers[idx].das_timer;
        let mut arr_timer = self.controllers[idx].arr_timer;
        let mut shifted_initial = self.controllers[idx].shifted_initial;
        if dir != 0 {
            if !shifted_initial {
                moved |= self.try_shift(idx, dir);
                shifted_initial = true;
            }
            das_timer += dt_ms;
            if das_timer >= self.settings.das as f32 {
                arr_timer += dt_ms;
                let step = self.settings.arr.max(1) as f32;
                while arr_timer >= step {
                    if !self.try_shift(idx, dir) {
                        break;
                    }
                    moved = true;
                    arr_timer -= step;
                }
            }
        } else {
            das_timer = 0.0;
            arr_timer = 0.0;
            shifted_initial = false;
        }
        self.controllers[idx].das_timer = das_timer;
        self.controllers[idx].arr_timer = arr_timer;
        self.controllers[idx].shifted_initial = shifted_initial;

        if inputs.hold {
            self.try_hold(idx);
        }

        // Gravity / soft drop
        let drop_speed = if inputs.soft_drop {
            self.settings.soft_drop.factor()
        } else {
            1.0
        };
        self.fall_accum[idx] += dt_ms * drop_speed;
        while self.fall_accum[idx] >= self.gravity_ms {
            if !self.try_fall(idx) {
                break;
            }
            self.fall_accum[idx] -= self.gravity_ms;
        }

        let on_ground = {
            let test = ActivePiece {
                y: self.players[idx].active.y - 1,
                ..self.players[idx].active.clone()
            };
            self.players[idx].board.collision(&test)
        };

        let piece = &mut self.players[idx].active;
        if rotated || moved {
            if on_ground && piece.move_resets > 0 {
                piece.lock_timer = LOCK_DELAY_MS;
                piece.move_resets -= 1;
            }
        }

        if on_ground {
            piece.lock_timer -= dt_ms;
            if piece.lock_timer <= 0.0 {
                let (cleared, t_spin) = self.players[idx].lock_piece();
                self.on_piece_locked(idx, cleared, t_spin);
                self.fall_accum[idx] = 0.0;
            }
        } else {
            piece.lock_timer = LOCK_DELAY_MS;
            piece.move_resets = 15;
        }
    }

    fn try_fall(&mut self, idx: usize) -> bool {
        let test = ActivePiece {
            y: self.players[idx].active.y - 1,
            ..self.players[idx].active.clone()
        };
        if self.players[idx].board.collision(&test) {
            return false;
        }
        self.players[idx].active = test;
        true
    }

    fn try_shift(&mut self, idx: usize, dir: i32) -> bool {
        let test = ActivePiece {
            x: self.players[idx].active.x + dir,
            ..self.players[idx].active.clone()
        };
        if self.players[idx].board.collision(&test) {
            return false;
        }
        self.players[idx].active = test;
        true
    }

    fn try_rotate(&mut self, idx: usize, cw: bool, double: bool) -> bool {
        if double {
            // Apply two sequential 90-degree rotations with kicks.
            let first = self.try_rotate(idx, cw, false);
            let second = self.try_rotate(idx, cw, false);
            return first || second;
        }
        let from = self.players[idx].active.rotation;
        let to = if cw { from.rotate_cw() } else { from.rotate_ccw() };
        let kicks = KickTable::kicks(self.players[idx].active.piece, from, to);
        for (_kick_idx, (dx, dy)) in kicks.iter().enumerate() {
            let test = ActivePiece {
                rotation: to,
                x: self.players[idx].active.x + dx,
                y: self.players[idx].active.y + dy,
                ..self.players[idx].active.clone()
            };
            if !self.players[idx].board.collision(&test) {
                self.players[idx].active = test;
                self.players[idx].last_action_was_t_spin =
                    self.players[idx].active.piece == Tetromino::T;
                return true;
            }
        }
        false
    }

    fn try_hold(&mut self, idx: usize) {
        if self.players[idx].held_on_turn {
            return;
        }
        let current = self.players[idx].active.piece;
        if let Some(held) = self.players[idx].hold {
            self.players[idx].active = ActivePiece::new(held);
            self.players[idx].hold = Some(current);
        } else {
            self.players[idx].hold = Some(current);
            self.players[idx].spawn_next();
        }
        self.players[idx].held_on_turn = true;
    }

    fn ghost(&self, idx: usize) -> Vec<Point> {
        let mut ghost = self.players[idx].active.clone();
        // Drop straight down until collision.
        loop {
            let test = ActivePiece {
                y: ghost.y - 1,
                ..ghost.clone()
            };
            if self.players[idx].board.collision(&test) {
                break;
            }
            ghost = test;
            if ghost.y <= 0 {
                break;
            }
        }
        ghost
            .blocks()
            .iter()
            .filter_map(|b| {
                let gy = ghost.y + b.y as i32;
                if (0..VISIBLE_HEIGHT as i32).contains(&gy) {
                    Some(Point {
                        x: ghost.x as i8 + b.x,
                        y: gy as i8,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn snapshot(&self) -> FrameView {
        let mut players = Vec::new();
        for idx in 0..2 {
            let mut field = Vec::with_capacity(WIDTH * VISIBLE_HEIGHT);
            for y in 0..VISIBLE_HEIGHT {
                for x in 0..WIDTH {
                    field.push(self.players[idx].cells(y, x));
                }
            }
            let active = self.players[idx]
                .active
                .blocks()
                .iter()
                .filter_map(|b| {
                    let ay = self.players[idx].active.y + b.y as i32;
                    if (0..VISIBLE_HEIGHT as i32).contains(&ay) {
                        Some(Point {
                            x: self.players[idx].active.x as i8 + b.x,
                            y: ay as i8,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let ghost = if self.settings.ghost_enabled {
                self.ghost(idx)
            } else {
                Vec::new()
            };
            let next = self.players[idx]
                .queue
                .iter()
                .copied()
                .map(|p| p.color_id())
                .collect();
            let next_blocks = self.players[idx]
                .queue
                .iter()
                .map(|p| spawn_blocks(*p).to_vec())
                .collect();
            let hold_blocks = self.players[idx].hold.map(|p| spawn_blocks(p).to_vec());
            let stats = &self.stats[idx];
            let time_s = if stats.time_ms > 0.0 { stats.time_ms / 1000.0 } else { 0.0 };
            let pps = if time_s > 0.0 {
                stats.pieces as f32 / time_s
            } else {
                0.0
            };
            let kpp = if stats.pieces > 0 {
                stats.keys as f32 / stats.pieces as f32
            } else {
                0.0
            };
            players.push(PlayerView {
                field,
                active,
                active_color: self.players[idx].active.piece.color_id(),
                active_piece: self.players[idx].active.piece.color_id(),
                active_rotation: format!("{:?}", self.players[idx].active.rotation),
                ghost,
                hold: self.players[idx].hold.map(|p| p.color_id()),
                hold_blocks,
                hold_color_id: self.players[idx].hold.map(|p| p.color_id()),
                next,
                next_blocks,
                topped_out: self.players[idx].topped_out,
                stats: PlayerStatsView {
                    time_ms: stats.time_ms,
                    pieces: stats.pieces,
                    keys: stats.keys,
                    attack: stats.attack,
                    finesse: stats.finesse,
                    pps,
                    kpp,
                    lines_sent: stats.lines_sent,
                    pending_garbage: self.players[idx].pending_garbage,
                },
            });
        }
        FrameView {
            players,
            settings: self.settings.clone(),
        }
    }

    fn tbp_start(&self, idx: usize) -> Result<frontend_msg::Start, String> {
        let player = self.players.get(idx).ok_or("invalid player index")?;
        let mut board_rows: Vec<Vec<Option<char>>> = Vec::with_capacity(TOTAL_HEIGHT);
        for y in 0..TOTAL_HEIGHT {
            let mut row = Vec::with_capacity(WIDTH);
            for x in 0..WIDTH {
                row.push(color_to_cell_char(player.board.cells[y][x]));
            }
            board_rows.push(row);
        }

        let mut queue: Vec<MaybeUnknown<tbp_data::Piece>> = Vec::new();
        queue.push(MaybeUnknown::Known(player.active.piece.into()));
        queue.extend(
            player
                .queue
                .iter()
                .copied()
                .map(|p| MaybeUnknown::Known(p.into())),
        );

        let randomizer = match player.randomizer_kind {
            RandomizerKind::SevenBag | RandomizerKind::LoveTris => {
                if let Some(bag) = player.randomizer.bag_state() {
                    tbp_randomizer::RandomizerState::SevenBag(tbp_randomizer::SevenBag::new(
                        bag.into_iter().map(Into::into).collect(),
                    ))
                } else {
                    tbp_randomizer::RandomizerState::SevenBag(tbp_randomizer::SevenBag::new(
                        Vec::new(),
                    ))
                }
            }
            _ => tbp_randomizer::RandomizerState::Unknown,
        };

        let mut start = frontend_msg::Start::new(
            player.hold.map(|p| MaybeUnknown::Known(p.into())),
            queue,
            player.combo,
            player.back_to_back,
            board_rows,
        );
        start.randomizer = randomizer;
        Ok(start)
    }

    fn apply_tbp_move(
        &mut self,
        idx: usize,
        mv: tbp_data::Move,
    ) -> Result<AppliedMoveResult, String> {
        if idx >= self.players.len() {
            return Err("invalid player index".into());
        }
        if self.players[idx].topped_out {
            return Err("player topped out".into());
        }
        let desired_piece: Tetromino = mv
            .location
            .kind
            .clone()
            .known()
            .ok_or("unknown piece in move")?
            .into();
        {
            let player = &mut self.players[idx];
            if desired_piece != player.active.piece {
                let queue_front = player.queue.get(0).copied();
                if let Some(hold) = player.hold {
                    if hold == desired_piece {
                        let previous = player.active.piece;
                        player.active = ActivePiece::new(desired_piece);
                        player.hold = Some(previous);
                        player.held_on_turn = true;
                    } else if queue_front == Some(desired_piece) && !player.held_on_turn {
                        // Bot used hold to skip to the next piece.
                        player.hold = Some(player.active.piece);
                        player.active = ActivePiece::new(desired_piece);
                        player.queue.remove(0);
                        player.refill_queue();
                        player.held_on_turn = true;
                    } else {
                        return Err("move piece not available (not current or held)".into());
                    }
                } else if queue_front == Some(desired_piece) && !player.held_on_turn {
                    // Hold was empty; bot is effectively holding current and using next.
                    player.hold = Some(player.active.piece);
                    player.active = ActivePiece::new(desired_piece);
                    player.queue.remove(0);
                    player.refill_queue();
                    player.held_on_turn = true;
                } else {
                    return Err("move piece not available (hold empty)".into());
                }
            }

            let orientation = mv
                .location
                .orientation
                .clone()
                .known()
                .ok_or("unknown orientation in move")?;
            player.active.rotation = from_tbp_orientation(orientation);
            player.active.x = mv.location.x as i32;
            player.active.y = mv.location.y as i32;
            if player.active.piece == Tetromino::I
                && (player.active.rotation == Rotation::Right
                    || player.active.rotation == Rotation::Reverse)
            {
                // Our I vertical column is shifted +1 relative to TBP coords; align to TBP pivot.
                player.active.x -= 1;
            }
            if player.board.collision(&player.active) {
                // If the suggested y collides, try dropping to the lowest legal height for this x/rotation.
                let shape = player.active.blocks();
                if let Some(drop_y) = player.board.lowest_drop_height(player.active.x, &shape) {
                    player.active.y = drop_y;
                    if player.board.collision(&player.active) {
                        return Err("placement collides with board".into());
                    }
                } else {
                    return Err("placement collides with board".into());
                }
            }
        }

        let (cleared, t_spin);
        {
            let player = &mut self.players[idx];
            let res = player.lock_piece();
            cleared = res.0;
            t_spin = res.1;
        }
        self.on_piece_locked(idx, cleared, t_spin);
        self.fall_accum[idx] = 0.0;

        let (topped_out, active_piece, new_queue_piece, combo, back_to_back) = {
            let player = &self.players[idx];
            (
                player.topped_out,
                if player.topped_out {
                    None
                } else {
                    Some(player.active.piece.into())
                },
                player
                    .last_refill_added
                    .map(Into::into)
                    .or_else(|| player.queue.last().copied().map(Into::into)),
                player.combo,
                player.back_to_back,
            )
        };

        Ok(AppliedMoveResult {
            lines_cleared: cleared,
            topped_out,
            active_piece,
            new_queue_piece,
            combo,
            back_to_back,
        })
    }

    fn set_randomizer(&mut self, player: usize, kind: RandomizerKind) {
        if let Some(p) = self.players.get_mut(player) {
            p.set_randomizer(kind);
        }
    }
}

impl Player {
    fn cells(&self, row: usize, col: usize) -> u8 {
        self.board.cells[row][col]
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AttackTable {
    pub _0_lines: u8,
    pub _1_line_single: u8,
    pub _2_lines_double: u8,
    pub _3_lines_triple: u8,
    pub _4_lines: u8,
    pub t_spin_double: u8,
    pub t_spin_triple: u8,
    pub t_spin_single: u8,
    pub t_spin_mini_single: u8,
    pub perfect_clear: u8,
    pub back_to_back_bonus: u8,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ComboTable {
    pub c0: u8,
    pub c1: u8,
    pub c2: u8,
    pub c3: u8,
    pub c4: u8,
    pub c5: u8,
    pub c6: u8,
    pub c7: u8,
    pub c8: u8,
    pub c9: u8,
    pub c10: u8,
    pub c11: u8,
    pub c12_plus: u8,
}

fn default_attack_table() -> AttackTable {
    AttackTable {
        _0_lines: 0,
        _1_line_single: 0,
        _2_lines_double: 1,
        _3_lines_triple: 2,
        _4_lines: 4,
        t_spin_double: 4,      // send 4 lines
        t_spin_triple: 6,      // send 6 lines
        t_spin_single: 2,      // send 2 lines
        t_spin_mini_single: 0, // unchanged
        perfect_clear: 10,
        back_to_back_bonus: 1,
    }
}

fn default_combo_table() -> ComboTable {
    ComboTable {
        c0: 0,
        c1: 0,
        c2: 1,
        c3: 1,
        c4: 1,
        c5: 2,
        c6: 2,
        c7: 3,
        c8: 3,
        c9: 4,
        c10: 4,
        c11: 4,
        c12_plus: 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sort_points(mut pts: Vec<Point>) -> Vec<Point> {
        pts.sort_by_key(|p| (p.x, p.y));
        pts
    }

    #[test]
    fn srs_shapes_match_reference() {
        let expected = |piece, pts: &[(i8, i8)]| {
            // Spawn orientation only; rotations derive from rotate_point.
            assert_eq!(
                sort_points(
                    shape_blocks(piece, Rotation::Spawn)
                        .iter()
                        .map(|p| Point { x: p.x, y: p.y })
                        .collect()
                ),
                sort_points(pts.iter().map(|(x, y)| Point { x: *x, y: *y }).collect())
            );
        };
        expected(Tetromino::S, &[(-1, 0), (0, 0), (0, 1), (1, 1)]);
        expected(Tetromino::Z, &[(-1, 1), (0, 1), (0, 0), (1, 0)]);
    }

    #[test]
    fn srs_kicks_match_reference_jlstz_and_i() {
        // JLSTZ 0->R: (0,0), (-1,0), (-1,1), (0,-2), (-1,-2)
        let kicks_j = KickTable::kicks(Tetromino::J, Rotation::Spawn, Rotation::Right);
        assert_eq!(kicks_j, vec![(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)]);
        let kicks_j_back = KickTable::kicks(Tetromino::J, Rotation::Right, Rotation::Spawn);
        assert_eq!(kicks_j_back, vec![(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)]);

        let kicks_i = KickTable::kicks(Tetromino::I, Rotation::Spawn, Rotation::Right);
        assert_eq!(kicks_i, vec![(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)]);
        let kicks_i_back = KickTable::kicks(Tetromino::I, Rotation::Right, Rotation::Spawn);
        assert_eq!(kicks_i_back, vec![(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]);
    }
}

#[wasm_bindgen]
pub struct GameClient {
    versus: Versus,
    input_state: InputState,
}

#[wasm_bindgen]
impl GameClient {
    #[wasm_bindgen(constructor)]
    pub fn new(settings: JsValue, bot_pps: f32, randomizers: JsValue) -> Result<GameClient, JsValue> {
        let settings: GameSettings = from_value(settings).unwrap_or_default();
        let randomizers: [RandomizerKind; 2] = from_value(randomizers)
            .unwrap_or([RandomizerKind::SevenBag, RandomizerKind::SevenBag]);
        let versus = Versus::new(settings, BotConfig { pps: bot_pps }, randomizers);
        Ok(Self {
            versus,
            input_state: InputState::default(),
        })
    }

    #[wasm_bindgen(js_name = tick)]
    pub fn tick(&mut self, dt_ms: f32) -> Result<JsValue, JsValue> {
        let frame: InputFrame = self.input_state.clone().into();
        self.versus.tick(dt_ms, frame);
        to_value(&self.versus.snapshot()).map_err(|e| e.into())
    }

    #[wasm_bindgen(js_name = setInput)]
    pub fn set_input(&mut self, input: JsValue) -> Result<(), JsValue> {
        let parsed: InputFrame = from_value(input)?;
        self.input_state = InputState {
            left: parsed.left,
            right: parsed.right,
            soft_drop: parsed.soft_drop,
            hard_drop: parsed.hard_drop,
            rotate_ccw: parsed.rotate_ccw,
            rotate_cw: parsed.rotate_cw,
            rotate_180: parsed.rotate_180,
            hold: parsed.hold,
        };
        Ok(())
    }

    #[wasm_bindgen(js_name = setRandomizer)]
    pub fn set_randomizer(&mut self, player: usize, kind: JsValue) -> Result<(), JsValue> {
        let parsed: RandomizerKind = from_value(kind)?;
        self.versus.set_randomizer(player, parsed);
        Ok(())
    }

    #[wasm_bindgen(js_name = setInternalBotEnabled)]
    pub fn set_internal_bot_enabled(&mut self, enabled: bool) {
        self.versus.use_internal_bot = enabled;
        if enabled {
            log("[bot] internal bot enabled (fallback)");
        } else {
            log("[bot] internal bot disabled (awaiting external plans)");
        }
    }

    #[wasm_bindgen(js_name = tbpStart)]
    pub fn tbp_start(&self, player: usize) -> Result<JsValue, JsValue> {
        let start = self
            .versus
            .tbp_start(player)
            .map_err(|e| JsValue::from_str(&e))?;
        to_value(&start).map_err(|e| e.into())
    }

    #[wasm_bindgen(js_name = tbpApplyMove)]
    pub fn tbp_apply_move(&mut self, player: usize, mv: JsValue) -> Result<JsValue, JsValue> {
        let parsed: tbp_data::Move = from_value(mv)?;
        let result = self
            .versus
            .apply_tbp_move(player, parsed)
            .map_err(|e| JsValue::from_str(&e))?;
        to_value(&result).map_err(|e| e.into())
    }

    #[wasm_bindgen(js_name = tbpStartJson)]
    pub fn tbp_start_json(&self, player: usize) -> Result<String, JsValue> {
        let start = self
            .versus
            .tbp_start(player)
            .map_err(|e| JsValue::from_str(&e))?;
        serde_json::to_string(&start).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
