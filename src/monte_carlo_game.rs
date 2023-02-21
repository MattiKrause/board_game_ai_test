use std::fmt::Debug;
use std::hash::Hash;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Winner {
    WIN = 0, TIE = 1
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
#[repr(u8)]
pub enum TwoPlayer {
    P1 = 1, P2 = 0
}

impl TwoPlayer {
    pub fn next(self) -> TwoPlayer {
        match self {
            TwoPlayer::P1 => TwoPlayer::P2,
            TwoPlayer::P2 => TwoPlayer::P1
        }
    }
}


pub trait MonteCarloGame: Clone + Hash + Eq + Debug{
    type MOVE: Copy + Debug + PartialEq + Eq;
    type MOVES<'s>: IntoIterator<Item = Self::MOVE> + 's where Self: 's;

    fn new() -> Self;
    fn moves(&self) -> Self::MOVES<'_>;
    fn make_move(&self, m: &Self::MOVE) -> Result<(Self, Option<Winner>), ()>;
    fn player(&self) -> TwoPlayer;
}