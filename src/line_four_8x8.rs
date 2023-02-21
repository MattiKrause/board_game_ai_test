use std::fmt::{Debug, Formatter, Write};
use std::marker::PhantomData;
use crate::{MonteCarloGame, TwoPlayer, Winner};

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct LineFour8x8 {
    set_by_p1: u64,
    set_by_p2: u64,
    player: TwoPlayer
}

macro_rules! column_index {
    ($name: ident, $($column: ident = $num: literal),* ) => {
        #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
        #[repr(u8)]
        pub enum $name {
            $($column = $num),*
        }
        impl TryFrom<u64> for $name {
            type Error = ();
            fn try_from(num: u64) ->  Result<Self, ()> {
                match num {
                    $($num => { Ok($name::$column) })*
                    _ => Err(())
                }
            }
        }
        impl TryFrom<u8> for $name {
            type Error = ();
            fn try_from(num: u8) -> Result<Self, ()> { Self::try_from(num as u64) }
        }
        impl TryFrom<u32> for $name {
            type Error = ();
            fn try_from(num: u32) -> Result<Self, ()> { Self::try_from(num as u64) }
        }
    };
}
column_index!(LineFour8x8Index, I0 = 0, I1 = 1, I2 = 2, I3 = 3, I4 = 4, I5 = 5, I6 = 6, I7 = 7);

pub struct AdHocMoves<M: TryFrom<u8>> {
    remaining: u8,
    conv: PhantomData<*const M>
}

impl <M: TryFrom<u8>> Iterator for AdHocMoves<M> {
    type Item = M;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.remaining.trailing_zeros();
        if next == 8 {
            None
        } else {
            self.remaining ^= 1 << next;
            M::try_from(next as u8).ok()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining.count_ones() as usize, Some(self.remaining.count_ones() as usize))
    }
}

impl LineFour8x8 {
    fn won(board: u64) -> bool {
        const WON_ROW: u64 = 0xF8F8_F8F8_F8F8_F8F8;
        if (board & board << 01 & board << 02 & board << 03) & WON_ROW > 0 {
            return true
        }
        const WON_COLUMN: u64 = 0xFFFF_FFFF_FF00_0000;
        if (board & board << 08 & board << 16 & board << 24) & WON_COLUMN > 0 {
            return true;
        }
        const WON_LBRT: u64 = 0x0000_00F8_F8F8_F8F8;
        if (board & board >> 07 & board >> 14 & board >> 21) & WON_LBRT > 0 {
            return true;
        }
        const WON_LTRB: u64 = 0xF8F8_F8F8_F800_0000;
        if (board & board << 9 & board << 18 & board << 27) & WON_LTRB > 0 {
            return true;
        }
        return false;
    }
}

impl MonteCarloGame for LineFour8x8 {
    type MOVE = LineFour8x8Index;
    type MOVES<'s> = AdHocMoves<Self::MOVE>;

    fn new() -> Self {
        Self {
            set_by_p1: 0,
            set_by_p2: 0,
            player: TwoPlayer::P1
        }
    }

    fn moves(&self) -> Self::MOVES<'_> {
        let all_set = self.set_by_p2 | self.set_by_p1;
        let all_unset = !all_set;
        let unset_top_row = all_unset >> 8 * 7;
        return AdHocMoves {
            remaining: unset_top_row as u8,
            conv: Default::default()
        }
    }

    fn make_move(&self, m: &Self::MOVE) -> Result<(Self, Option<Winner>), ()> {
        const COLUMN_MASK: u64 = 0x01010101_01010101;
        let index = *m as u8 as u32;
        let column_mask = COLUMN_MASK << index;
        let all_set = self.set_by_p1 | self.set_by_p2;
        let set_index = (column_mask^(all_set & column_mask)).trailing_zeros();
        if set_index >= 64 {
            return Err(())
        }
        let pnum = match self.player() {
            TwoPlayer::P1 => 1,
            TwoPlayer::P2 => 0,
        };
        let new_p1 = self.set_by_p1 | pnum << set_index;
        let new_p2 = self.set_by_p2 | (pnum ^ 1) << set_index;
        let check_board = match self.player() {
            TwoPlayer::P1 => new_p1,
            TwoPlayer::P2 => new_p2,
        };
        let (new_player, winner) = if Self::won(check_board) {
            (self.player(), Some(Winner::WIN))
        } else if new_p2 | new_p1 == u64::MAX {
            (self.player(), Some(Winner::TIE))
        } else {
            (self.player().next(), None)
        };
        let new_state = Self {
            set_by_p1: new_p1,
            set_by_p2: new_p2,
            player: new_player
        };
        Ok((new_state, winner))
    }

    fn player(&self) -> TwoPlayer {
        self.player
    }
}

impl Debug for LineFour8x8 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for r in (0..8).rev() {
            for c in 0..8 {
                f.write_char('|')?;
                let char = if (self.set_by_p1 >> (r * 8 + c)) & 1 == 1 {
                    'x'
                } else if (self.set_by_p2 >> (r * 8 + c)) & 1 == 1 {
                    'o'
                } else {
                    ' '
                };
                f.write_char(char)?;
            }
            f.write_char('|')?;
            f.write_char('\n')?;
        }
        return Ok(())
    }
}