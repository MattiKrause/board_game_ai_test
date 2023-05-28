pub trait WinReducerFactory {
    type WR: WinReducer;
    fn create(&self, score: f64) -> Self::WR;
}

pub trait WinReducer: Sized {
    fn deteriorate(&mut self, child_count: usize) { self.get_and_deteriorate(child_count); }
    fn get_and_deteriorate(&mut self, child_count: usize) -> f64;
}

#[derive(Copy, Clone)]
pub struct WinIdentFactory;
pub struct WinIdent(f64);

impl WinReducerFactory for WinIdentFactory {
    type WR = WinIdent;

    fn create(&self, score: f64) -> Self::WR {
        WinIdent(score)
    }
}

impl WinReducer for WinIdent {
    fn get_and_deteriorate(&mut self, _child_count: usize) -> f64 {
        self.0
    }
}

#[derive(Copy, Clone, Debug)]
pub struct WinFactorReduceFactory {
    pub by: f64
}
pub struct WinFactorReduce {
    score: f64,
    by: f64,
}
impl WinReducerFactory for WinFactorReduceFactory {
    type WR = WinFactorReduce;

    fn create(&self, score: f64) -> Self::WR {
        WinFactorReduce {
            score,
            by: self.by
        }
    }
}
impl WinReducer for WinFactorReduce {
    fn get_and_deteriorate(&mut self, _child_count: usize) -> f64 {
        let score = self.score;
        self.score *= self.by;
        score
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ScoreAveragerFactory;
pub struct ScoreAveragerReduce(f64);

impl WinReducerFactory for ScoreAveragerFactory {
    type WR = ScoreAveragerReduce;

    fn create(&self, score: f64) -> Self::WR {
        ScoreAveragerReduce(score)
    }
}

impl WinReducer for ScoreAveragerReduce {
    fn get_and_deteriorate(&mut self, child_count: usize) -> f64 {
        self.0 /= child_count as f64;
        self.0
    }
}