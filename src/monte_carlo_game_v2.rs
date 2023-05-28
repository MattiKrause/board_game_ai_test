use std::fmt::Debug;
use std::hash::Hash;
use crate::monte_carlo_game::MonteCarloGame;

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum GameState {
    Continue,
    Finished
}

pub trait MonteCarloGameND: Clone + Hash + Eq + Debug{
    type MOVE: Clone + Debug + PartialEq + Eq;
    type Outcome: Clone + Debug + PartialEq + Eq;
    type MOVES<'s>: IntoIterator<Item = Self::MOVE> + 's where Self: 's;
    type Outcomes<'s>: IntoIterator<Item = (Self::Outcome, f64)> + 's where Self: 's;

    fn new() -> Self;
    fn moves(&self) -> Self::MOVES<'_>;
    fn get_outcomes(&self, m: &Self::MOVE) -> Result<Self::Outcomes<'_>, ()>;

    fn make_move(&self, m: &Self::MOVE, e: &Self::Outcome) -> Result<(Self, GameState), ()>;
}

impl <T: MonteCarloGame> MonteCarloGameND for T {
    type MOVE = T::MOVE;
    type Outcome = ();
    type MOVES<'s> where Self: 's = T::MOVES<'s>;
    type Outcomes<'s> where Self: 's = std::iter::Once<((), f64)>;

    fn new() -> Self {
        T::new()
    }

    fn moves(&self) -> Self::MOVES<'_> {
        T::moves(self)
    }

    fn get_outcomes(&self, _: &Self::MOVE) -> Result<Self::Outcomes<'_>, ()> {
        Ok(std::iter::once(((), 1.0)))
    }

    fn make_move(&self, m: &Self::MOVE, _: &()) -> Result<(Self, GameState), ()> {
        self.make_move(m).map(|(state, winner)| {
            let gs = match winner {
                Some(_) => GameState::Finished,
                None => GameState::Continue
            };
            (state, gs)
        })
    }
}