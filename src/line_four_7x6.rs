use std::fmt::{Debug, Formatter, Write};
use crate::monte_carlo_game::{MonteCarloGame, TwoPlayer, Winner};

#[derive(Copy, Clone, Hash, Eq,  PartialEq)]
pub struct LineFourGame {
    set_by_p1: u64,
    set_by_p2: u64,
    turn: TwoPlayer
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum LineFourIndex {
    I0 = 0, I1 = 1, I2 = 2, I3 = 3, I4 = 4, I5 = 5, I6 = 6
}
impl TryFrom<u32> for LineFourIndex {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::I0),
            1 => Ok(Self::I1),
            2 => Ok(Self::I2),
            3 => Ok(Self::I3),
            4 => Ok(Self::I4),
            5 => Ok(Self::I5),
            6 => Ok(Self::I6),
            _ => Err(())
        }
    }
}

impl LineFourGame {
    pub fn set_at_index(&mut self, index: &LineFourIndex) -> Result<Option<Winner>, ()> {
        let index: u8 = *index as u8;
        let mut set_index = (((self.set_by_p1 | self.set_by_p2) >> index * 6) & 0b111111).trailing_ones();
        if set_index >= 6 { return Err(()) }
        set_index += (index as u32) * 6;
        let pnum = self.turn as u8 as u64;
        self.set_by_p1 |= pnum << set_index;
        self.set_by_p2 |= (pnum ^ 0b1) << set_index;
        let board = if pnum == 1 { self.set_by_p1 } else { self.set_by_p2 };
        const TIE: u64 = 0b111111_111111_111111_111111_111111_111111_111111;
        if Self::has_won_in(board) {
            Ok(Some(Winner::WIN))
        } else if self.set_by_p2 | self.set_by_p1 == TIE {
            return Ok(Some(Winner::TIE))
        } else {
            self.turn = self.turn.next();
            Ok(None)
        }
    }

    pub fn has_won_in(board: u64) -> bool {
        const VERTICAL_WON: u64 = 0b111000_111000_111000_111000_111000_111000_111000;
        if (board & board << 01 & board << 02 & board << 03) & VERTICAL_WON > 0 {
            return true;
        }
        const HORIZONTAL_WON: u64 = 0b1111111_1111111_1111111_1111111_000000_000000_000000;
        if (board & board << 06 & board << 12 & board << 18) & HORIZONTAL_WON > 0 {
            return true;
        }
        const LTRB_DIAGONAL: u64 = VERTICAL_WON & HORIZONTAL_WON;
        if (board & board << 07 & board << 14 & board << 21) & LTRB_DIAGONAL > 0 {
            return true
        }
        const LBRT_DIAGONAL: u64 = 0b000111_000111_000111_000111_000000_000000_000000;
        if (board & board << 05 & board << 10 & board << 15) & LBRT_DIAGONAL > 0 {
            return true
        }
        return false
    }
}

const fn line_four_move_set() -> [[LineFourIndex; 7]; 128] {
    let mut i = 0u16;
    let mut res = [[LineFourIndex::I0; 7]; 128];
    while i <= 0b0111_1111 {
        let mut inner = [LineFourIndex::I0; 7];
        let mut next_idx = 0;
        let mut j = 0;
        macro_rules! set_lfi {
            ($lfi: ident) => {
                if i >> j & 1 == 1 {
                    inner[next_idx] = LineFourIndex::$lfi;
                    next_idx += 1;
                }
                j += 1;
            };
            ($lfi: ident, $($rest: ident),*) => {
                set_lfi!($lfi);
                set_lfi!($($rest),*);
            }
        }
        set_lfi!(I0, I1, I2, I3, I4, I5, I6);
        res[i as usize] = inner;
        i += 1;
    }
    return res
}

static VALID_MOVES: [[LineFourIndex; 7]; 128] = line_four_move_set();

impl MonteCarloGame for  LineFourGame {
    type MOVE = LineFourIndex;
    type MOVES<'s> = std::iter::Cloned<std::slice::Iter<'static, LineFourIndex>>;

    fn new() -> Self {
        Self {
            set_by_p1: 0,
            set_by_p2: 0,
            turn: TwoPlayer::P1
        }
    }

    fn moves<'s>(&'s self) -> Self::MOVES<'s> {
        let used = self.set_by_p1 | self.set_by_p2;
        let mut viable = 0u8;
        for i in 0..7 {
            let mask = 1 << i;
            let shift_by = 5 * (i + 1);
            let column = used >> shift_by;
            let column_top = column & mask;
            let column_free = column_top ^ mask;
            viable |= column_free as u8;
        }
        let moves = &VALID_MOVES[viable as usize][0..(viable.count_ones() as usize)];
        moves.iter().cloned()
    }

    fn make_move(&self, m: &Self::MOVE) -> Result<(Self, Option<Winner>), ()> {
        let mut new = self.clone();
        new.set_at_index(m).map(|res| (new, res))
    }

    fn player(&self) -> TwoPlayer {
        self.turn
    }
}

impl Debug for LineFourGame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        fn get_char(state: &LineFourGame, index: u8) -> char {
            if (state.set_by_p1 >> index) & 1 == 1 {
                'x'
            } else if (state.set_by_p2 >> index) & 1 == 1 {
                'o'
            } else {
                ' '
            }
        }
        for i in (0..6).rev() {
            for j in 0..7 {
                f.write_char('|')?;
                f.write_char(get_char(self, j * 6 + i))?;
            }
            f.write_char('|')?;
            f.write_char('\n')?;
        }
        for _ in 0..15 {
            f.write_char('-')?;
        }
        return Ok(())
    }
}