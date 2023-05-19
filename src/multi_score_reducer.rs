use std::marker::PhantomData;
use std::ops::ControlFlow;
use crate::monte_carlo_game::{MonteCarloGame, Winner};
use crate::monte_carlo_win_reducer::{WinFactorReduceFactory, WinReducer, WinReducerFactory};

pub trait CheckWinMonteCarloGame: MonteCarloGame {
    fn win_state(&self) -> Option<Winner>;
}

pub trait MultiScoreReducerFactory<G> {
    type WR<'a>: ScoreReducer + 'a where Self: 'a;
    fn create<'wr>(&'wr self, game: &'_ G) -> Self::WR<'wr>;
}

pub trait ScoreReducer {
    fn next_score(&mut self, child_count: usize) -> f64;
}

pub trait ExecutionLimiterFactory<G> {
    type EL<'a>: ExecutionLimiter<G> + 'a where Self: 'a;

    fn create<'a>(&'a self) -> Self::EL<'a>;
}

pub trait ExecutionLimiter<G> {
    fn next(&mut self, child_count: usize) -> std::ops::ControlFlow<(), ()>;
    fn next_with_game(&mut self, child_count: usize, game: &G) -> std::ops::ControlFlow<(), ()> {
        self.next(child_count)
    }
}

pub trait GetMostExtremeSourceScore {
    type WR: WinReducer;
    fn get_most_extreme(&self) -> Self::WR;
}

#[derive(Copy, Clone)]
pub struct TwoScoreReducerFactory<F1, F2> {
    fac_1: F1, fac_2: F2
}

pub struct TwoScoreReducerExecutionLimiterFactory<F1, F2> {
    threshold: f64, fac: TwoScoreReducerFactory<F1, F2>
}

pub struct TwoScoreReducerExecutionLimiter<WR1, WR2> {
    wr1: WR1, wr2: WR2, treshold: f64
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

impl <F: WinReducerFactory> GetMostExtremeSourceScore for WinRewardInit<F> {
    type WR = F::WR;

    fn get_most_extreme(&self) -> Self::WR {
        if self.on_win.abs() > self.on_tie.abs() {
            self.f.create(self.on_win)
        } else {
            self.f.create(self.on_tie)
        }
    }
}

impl <F1, F2> TwoScoreReducerFactory<F1, F2> {
    pub(crate) fn new(fac_1: F1, fac_2: F2) -> Self {
        Self {
            fac_1,
            fac_2,
        }
    }

    pub fn limiter_from(self, threshold: f64) -> TwoScoreReducerExecutionLimiterFactory<F1, F2> {
        TwoScoreReducerExecutionLimiterFactory {
            threshold,
            fac: self,
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

impl <G, F1, F2> MultiScoreReducerFactory<G> for TwoScoreReducerExecutionLimiterFactory<F1, F2> where TwoScoreReducerFactory<F1, F2>: MultiScoreReducerFactory<G> {
    type WR<'a> where Self: 'a = <TwoScoreReducerFactory<F1, F2> as MultiScoreReducerFactory<G>>::WR<'a>;

    fn create<'wr>(&'wr self, game: &'_ G) -> Self::WR<'wr> {
        self.fac.create(game)
    }
}

impl <G, WR1: WinReducer, WR2: WinReducer> ExecutionLimiter<G> for TwoScoreReducerExecutionLimiter<WR1, WR2> {
    fn next(&mut self, child_count: usize) -> ControlFlow<(), ()> {
        let s1 = self.wr1.get_and_deteriorate(child_count).abs();
        let s2 = self.wr2.get_and_deteriorate(child_count).abs();
        if s1 < self.treshold || s2 < self.treshold {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    }
}

impl <G, F1: GetMostExtremeSourceScore, F2: GetMostExtremeSourceScore> ExecutionLimiterFactory<G> for TwoScoreReducerExecutionLimiterFactory<F1, F2> {
    type EL<'a> where Self: 'a = TwoScoreReducerExecutionLimiter<F1::WR, F2::WR>;

    fn create(&self) -> Self::EL<'_> {
        let extreme1 = self.fac.fac_1.get_most_extreme();
        let extreme2 = self.fac.fac_2.get_most_extreme();
        TwoScoreReducerExecutionLimiter {
            wr1: extreme1,
            wr2: extreme2,
            treshold: self.threshold,
        }
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