use std::fmt::{Debug, Formatter, Write};
use std::marker::PhantomData;
use crate::{MonteCarloGame, TwoPlayer, Winner};

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct LineFour8x8 {
    //Layout bytes = rows, first byte = first row, etc.
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
        // check vertical wins by ANDing each slot the three slots BEFORE it, only check the last 5 slots,
        // since the first 3 are polluted by the elements from the last row
        const WON_ROW: u64 = 0xF8_F8_F8_F8_F8_F8_F8_F8;
        if (board & board << 01 & board << 02 & board << 03) & WON_ROW > 0 {
            return true
        }

        // check horizontal wins by ANDing each row and the three row BEFORE it, which effectively
        // ANDs the slots of the column. The first three columns cannot be ANDed with four columns so
        // they are skipped
        const WON_COLUMN: u64 = 0xFF_FF_FF_FF_FF_00_00_00;
        if (board & board << 08 & board << 16 & board << 24) & WON_COLUMN > 0 {
            return true;
        }

        // check diagonal wins by ANDing each slot with the NEXT three slots in the
        // Left-Bottom to Right-Top diagonal line. Do not check the last rows because the rows above
        // them are not set, do not check the first the slots in each row, because they are polluted,
        // by the last slot in this row and the two rows above
        const WON_LBRT: u64 = 0x00_00_00_F8_F8_F8_F8_F8;
        if (board & board >> 07 & board >> 14 & board >> 21) & WON_LBRT > 0 {
            return true;
        }


        const WON_LTRB: u64 = 0xF8_F8_F8_F8_F8_00_00_00;
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
        //1 in the first slot of each row, effectively 1 in  all slots of the first column
        const COLUMN_MASK: u64 = 0x01_01_01_01_01_01_01_01;
        let index = *m as u8 as u32;

        // shift the 1s in the first column to the column into which the piece should be droppped
        let column_mask = COLUMN_MASK << index;
        let all_set = self.set_by_p1 | self.set_by_p2;

        // all already set slots in the column in which the new piece should be dropped
        let set_in_column = all_set & column_mask;
        let not_set_in_column = column_mask^set_in_column;

        // bit index of the new piece
        let set_index = not_set_in_column.trailing_zeros();

        if not_set_in_column == 0 {
            return Err(())
        }
        let pnum = match self.player() {
            TwoPlayer::P1 => 1,
            TwoPlayer::P2 => 0,
        };

        // set the piece in p1 if p1 is at turn an vise-versa
        let new_p1 = self.set_by_p1 | (pnum << set_index);
        let new_p2 = self.set_by_p2 | ((pnum ^ 1) << set_index);
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
            player: new_player,
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