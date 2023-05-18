use crate::monte_carlo_game::{MonteCarloGame, TwoPlayer, Winner};
use crate::multi_score_reducer::CheckWinMonteCarloGame;

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub struct TicTacToe {
    game_state: u32
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum TicTacToeMove {
    I1 = 0, I2 = 1, I3 = 2, I4 = 3, I5 = 4, I6 = 5, I7 = 6, I8 = 7, I9 = 8
}

pub struct TicTacToeMoves {
    remaining: u16
}

const BOARD_MASK: u32 = 0b111_111_111;
const fn pos_player1(board: u32) -> u32 { board & BOARD_MASK }
const fn pos_player2(board: u32) -> u32 { (board >> 9) & BOARD_MASK }
const fn get_player(board: u32) ->  TwoPlayer { if board >> 31 == 1 { TwoPlayer::P1 } else { TwoPlayer::P2 }}
const fn won_one_board(board: u16) -> bool {
    const LINE_WON: u16 = 0b100_100_100;
    let row_won = (board & board << 1 & board << 2) & LINE_WON;
    const COL_WON: u16 = 0b111_000_000;
    let col_won = (board & board << 3 & board << 6) & COL_WON;
    let dig1_won = (board >> 8 & board >> 4 & board) & 1;
    let dig2_won = (board & board >> 2 & board >> 4) & 0b000_000_100;
    (row_won | col_won | dig1_won | dig2_won) > 0
}

const fn is_tie(board: u32) -> bool {
    ((board >> 9) | board) & BOARD_MASK == BOARD_MASK
}

impl MonteCarloGame for TicTacToe {
    type MOVE = TicTacToeMove;
    type MOVES<'s> where Self: 's = TicTacToeMoves;

    fn new() -> Self {
        let me = Self { game_state: 0 | 1 << 31 };
        debug_assert!(get_player(me.game_state) == TwoPlayer::P1);
        me
    }

    fn moves(&self) -> Self::MOVES<'_> {
        let used_pos = pos_player1(self.game_state) | pos_player2(self.game_state);
        let unused = (!used_pos)  & BOARD_MASK;
        TicTacToeMoves { remaining: unused as u16 }
    }

    fn make_move(&self, m: &Self::MOVE) -> Result<(Self, Option<Winner>), ()> {
        let player_board_off = match get_player(self.game_state) {
            TwoPlayer::P1 => 0,
            TwoPlayer::P2 => 9
        };
        let m = *m as u32;
        let m_bit = 1 << (m + player_board_off);
        if self.game_state & m_bit > 0 {
            return Err(());
        }

        let new_board = self.game_state | m_bit;

        let (flip_player, winner) = if won_one_board(((new_board >> player_board_off) & BOARD_MASK)  as u16) {
            (0, Some(Winner::WIN))
        } else if is_tie(new_board) {
            (0, Some(Winner::TIE))
        } else {
            (1 << 31, None)
        };
        let new_board = new_board ^ flip_player;
        debug_assert!(winner != None || get_player(self.game_state).next() == get_player(new_board));
        Ok((Self { game_state: new_board }, winner))
    }

    fn player(&self) -> TwoPlayer {
        get_player(self.game_state)
    }
}

impl std::fmt::Debug for TicTacToe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        for row in 0..3 {
            for col in 0..3 {
                let write = if (self.game_state >> (row * 3 + col)) & 1 > 0 {
                    'x'
                } else if (self.game_state >> (row * 3 + col + 9)) & 1 > 0 {
                    'o'
                } else {
                    ' '
                };
                f.write_char(write)?;
            }
            f.write_char('\n')?;
        }
        Ok(())
    }
}

impl CheckWinMonteCarloGame for TicTacToe {
    fn win_state(&self) -> Option<Winner> {
        let off = match get_player(self.game_state) {
            TwoPlayer::P1 => 0,
            TwoPlayer::P2 => 9,
        };
        if won_one_board(((self.game_state >> off) & BOARD_MASK) as u16) {
            Some(Winner::WIN)
        } else if is_tie(self.game_state){
            Some(Winner::TIE)
        } else {
            None
        }
    }
}

impl Iterator for TicTacToeMoves {
    type Item = TicTacToeMove;

    fn next(&mut self) -> Option<Self::Item> {
        use TicTacToeMove::*;
        let next = self.remaining.trailing_zeros();
        if let Some(m) = TicTacToeMove::try_from(next).ok() {
            self.remaining ^= 1 << next;
            Some(m)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let num = (self.remaining as u32 & BOARD_MASK).count_ones() as usize;
        (num, Some(num))
    }
}

impl TryFrom<u32> for TicTacToeMove {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        use TicTacToeMove::*;
        let value = match value {
            0 => I1,
            1 => I2,
            2 => I3,
            3 => I4,
            4 => I5,
            5 => I6,
            6 => I7,
            7 => I8,
            8 => I9,
            _ => return Err(())
        };
        Ok(value)
    }
}