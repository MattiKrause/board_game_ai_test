use std::marker::PhantomData;
use crate::monte_carlo_game::{MonteCarloGame, Winner};
use crate::monte_carlo_win_reducer::{WinReducer, WinReducerFactory};

pub trait CheckWinMonteCarloGame: MonteCarloGame {
    fn win_state(&self) -> Option<Winner>;
}

pub trait MultiScoreReducerFactory<G: MonteCarloGame> {
    type WR<'a>: ScoreReducer + 'a where Self: 'a;
    fn create<'wr>(&'wr self, game: &'_ G) -> Self::WR<'wr>;
}

pub trait ScoreReducer: Sized {
    fn next_score(&mut self, child_count: usize) -> f64;
}

#[derive(Copy, Clone)]
pub struct TwoScoreReducerFactory<F1, F2> {
    fac_1: F1, fac_2: F2
}

pub struct TwoScoreReducer<R1, R2>(R1, R2, bool);

pub trait WinReducerFactoryWinInit {
    type WR: WinReducer;
    fn create(&self, end_result: Winner) -> Self::WR;
}

#[derive(Copy, Clone, Debug)]
pub struct WinRewardInit<F> {
    on_win: f64, on_tie: f64, f: F,
}

impl <F> WinRewardInit<F> {
    pub fn new(on_win: f64, on_tie: f64, f:F ) -> Self {
        Self {
            on_win,
            on_tie,
            f,
        }
    }
}

impl <F: WinReducerFactory> WinReducerFactoryWinInit for WinRewardInit<F> {
    type WR = F::WR;

    fn create(&self, end_result: Winner) -> Self::WR {
        let score = match end_result {
            Winner::WIN => self.on_win,
            Winner::TIE => self.on_tie
        };
        self.f.create(score)
    }
}

impl <F1, F2> TwoScoreReducerFactory<F1, F2> {
    pub(crate) fn new(fac_1: F1, fac_2: F2) -> Self {
        Self {
            fac_1,
            fac_2,
        }
    }
}

impl <G: CheckWinMonteCarloGame, F1: WinReducerFactoryWinInit, F2: WinReducerFactoryWinInit> MultiScoreReducerFactory<G> for TwoScoreReducerFactory<F1, F2> {
    type WR<'a> = TwoScoreReducer<F1::WR, F2::WR> where F1: 'a, F2: 'a;

    fn create<'wr>(&'wr self, game: &'_ G) -> Self::WR<'wr> {
        let win_state = game.win_state().expect("game not in a winning state");
        let r1 = self.fac_1.create(win_state);
        let r2 = self.fac_2.create(win_state);

        TwoScoreReducer(r1, r2, false)
    }
}

impl <R1: WinReducer, R2: WinReducer> ScoreReducer for TwoScoreReducer<R1, R2> {
    fn next_score(&mut self, child_count: usize) -> f64 {
        self.2 = !self.2;
        if !self.2 {
            self.1.deteriorate(child_count);
            self.0.get_and_deteriorate(child_count)
        } else {
            self.0.deteriorate(child_count);
            self.1.get_and_deteriorate(child_count)
        }
    }
}