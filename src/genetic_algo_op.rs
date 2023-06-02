use std::fs::{DirEntry, File, FileType, ReadDir};
use std::io::{stdin, stdout};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rand_distr::{LogNormal, SkewNormal};
use rayon::iter::IntoParallelRefIterator;
use crate::ai_infra::{GamePlayer, GameStrategy};
use crate::monte_carlo_win_reducer::{WinFactorReduce, WinFactorReduceFactory};
use crate::multi_score_reducer::{CheckWinMonteCarloGame, ExecutionLimiter, ScoreReducer, TwoScoreReducer, TwoScoreReducerExecutionLimiterFactory, TwoScoreReducerFactory, WinRewardInit};
use crate::old_monte_carlo::monte_carlo_main8::MonteCarloStrategyV8;
use crate::old_monte_carlo::monte_carlo_main::MonteLimit;
use rayon::prelude::*;
use crate::monte_carlo_game::{MonteCarloGame, TwoPlayer, Winner};
use serde::{Serialize, Deserialize};
use crate::monte_carlo_game_v2::MonteCarloGameND;
use crate::old_monte_carlo::monte_carlo_main7::MonteCarloStrategyV7;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RandomValues {
    c: f64,
    el_threshold: f64,
    degregation_1: f64,
    degregation_2: f64,
    win_reward_1: (f64, f64),
    win_reward_2: (f64, f64),
}

pub fn load_best_from_pop<G: MonteCarloGameND + CheckWinMonteCarloGame + 'static>(monte_limit: MonteLimit) -> Option<impl GamePlayer<G>> {
    let first = read_last_checkpoint()?.drain(..).next()?;
    let config = config_from_rv(monte_limit, &first);

    Some(MonteCarloStrategyV8::strategy_of(config))
}


pub fn opt<G: MonteCarloGame+ CheckWinMonteCarloGame + 'static>() {
    let monte_limit = MonteLimit::duration(100);
    let mut rng = SmallRng::from_entropy();
    let mut random_variants = move || {
        let c = rng.gen_range((0.0)..(10.0));
        let degregation_1 = rng.gen_range(0.0..(1.0));
        let degregation_2 = rng.gen_range(0.0..(1.0));
        let win_reward_1 = (rng.gen_range((-10.0)..(10.0)), rng.gen_range((-10.0)..(10.0)));
        let win_reward_2 = (rng.gen_range((-10.0)..(10.0)), rng.gen_range((-10.0)..(10.0)));
        let el_threshold = rng.gen_range((0.0)..(10.0));
        RandomValues {
            c,
            el_threshold,
            degregation_1,
            degregation_2,
            win_reward_1,
            win_reward_2,
        }
    };

    let mut candidates = match read_last_checkpoint() {
        Some(c) => c,
        None => {
            log::info!("no existing population found: starting new one");
            Vec::new()
        }
    };
    candidates.extend(std::iter::repeat_with(|| random_variants()).take(100usize.saturating_sub(candidates.len())));
    let mut candidates = candidates.into_iter().map(|rv| (rv, AtomicU32::new(0))).collect::<Vec<_>>();

    let mut last_saved = Instant::now();

    //916.1772972 s
    loop {
        let playoffs_start = Instant::now();
        do_random_playoffs::<G>(monte_limit, 1, &candidates);
        println!("commencing_mutation after {} seconds", playoffs_start.elapsed().as_secs_f64());

        candidates.sort_unstable_by_key(|(_, k)| k.load(Ordering::Relaxed));

        if last_saved.elapsed() > Duration::from_secs(60 * 20) {
            last_saved= Instant::now();
            let save = candidates.iter().rev().take(20).map(|(rv, _)| rv.clone()).collect::<Vec<_>>();
            let file = File::create(format!("checkpoint{}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()));

            match file {
                Ok(file) => {
                    let e = serde_json::to_writer(file, &save);
                    if let Err(e) = e {
                        eprintln!("failed to write json: {e}")
                    }
                }
                Err(e) => eprintln!("failed to write to file: {e}"),
            }
        }
        let first = candidates.iter().rev().take(10).map(|(rv, _)| rv.clone()).collect::<Vec<_>>();
        let random_pop = std::iter::repeat_with(|| random_variants()).take(10).collect::<Vec<_>>();
        let highest_value = candidates.iter_mut().fold(0, |acc, (_, rv)| {
            *rv.get_mut() += acc;
            *rv.get_mut() = rv.get_mut().pow(2);
            *rv.get_mut()
        });
        let mut rng = SmallRng::from_entropy();

        let mutants = std::iter::repeat_with(|| {
            let first = rng.gen_range(0..highest_value);
            let second = rng.gen_range(0..highest_value);

            let first = candidates.iter().map(|(rv, a)| (rv, a.load(Ordering::Relaxed))).find(|(_, c)| first < *c).unwrap();
            let second = candidates.iter().map(|(rv, a)| (rv, a.load(Ordering::Relaxed))).find(|(_, c)| second < *c).unwrap();

            let mut merge_factor = first.1 as f64 / (first.1 as f64 + second.1 as f64);
            merge_factor += 0.0;

            merge_rvs(first.0.clone(), second.0.clone(), merge_factor)
        })
            .take(80)
            .map(|rv| {
                let other = random_variants();
                let merge_factor = 1.0;
                merge_rvs(rv, other, merge_factor)
            })
            .collect::<Vec<_>>();

        candidates = first.into_iter()
            .chain(random_pop.into_iter())
            .chain(mutants.into_iter())
            .map(|rv| (rv, AtomicU32::new(0)))
            .collect::<Vec<_>>()
    }
}

fn merge_rvs(first: RandomValues, second: RandomValues, merge_factor: f64) -> RandomValues {
    let merge = |a: f64, b: f64| a * merge_factor + b * (1.0 - merge_factor);

    macro_rules! mval {
                ($n: ident) => {merge(first.$n, second.$n)};
            }

    RandomValues {
        c: mval!(c),
        el_threshold: mval!(el_threshold),
        degregation_1: mval!(degregation_1),
        degregation_2: mval!(degregation_2),
        win_reward_1: (merge(first.win_reward_1.0, second.win_reward_1.0), merge(first.win_reward_1.1, second.win_reward_1.1)),
        win_reward_2: (merge(first.win_reward_2.0, second.win_reward_2.0), merge(first.win_reward_2.1, second.win_reward_2.1)),
    }
}

fn read_last_checkpoint() -> Option<Vec<RandomValues>> {
    let dir = match std::fs::read_dir("./") {
        Ok(dir) => dir,
        Err(err) => {
            log::warn!("failed to open current dir: {err}");
            return None;
        }
    };
    let checkpoint_regex = regex::Regex::new("^checkpoint(\\d+)$").expect("failed to compile checkpoint regex");
    let file = dir.filter_map(|file| file.ok())
        .filter(|file| file.file_type().map_or(false, |t| t.is_file()))
        .filter_map(|file| file.file_name().into_string().map(|name| (file, name.clone())).ok())
        .filter_map(|(file, name)| checkpoint_regex.captures(name.as_str()).and_then(|c| c.get(1)).and_then(|m| m.as_str().parse::<u64>().ok()).map(|u| (file, u)))
        .max_by_key(|(_, written)| *written)
        .map(|(file, _)| file);
    let file = match file {
        Some(f) => f,
        None => {
            log::info!("no file to load from found");
            return None;
        }
    };
    let file_name = file.file_name();
    let mut file = match std::fs::File::open(file.path()) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("found checkpoint file {:?} but failed to read from it({e})", file_name);
            return None;
        }
    };
    match serde_json::from_reader::<_, Vec<RandomValues>>(&mut file) {
        Ok(r) => {
            log::info!("starting from checkpoint file {:?}", file_name);
            Some(r)
        },
        Err(e) => {
            log::warn!("found checkpoint file {:?} but failed to parse content({e})", file_name);
            None
        }
    }
}

fn config_from_rv(monte_limit: MonteLimit, RandomValues{ c, el_threshold, degregation_1, degregation_2, win_reward_1, win_reward_2 }: &RandomValues) -> (MonteLimit, f64, TwoScoreReducerExecutionLimiterFactory<WinRewardInit<WinFactorReduceFactory>, WinRewardInit<WinFactorReduceFactory>>, Option<[u8; 32]>) {
    let wri1 = WinRewardInit::new(win_reward_1.0, win_reward_1.1, WinFactorReduceFactory { by: *degregation_1 });
    let wri2 = WinRewardInit::new(win_reward_2.0, win_reward_2.1, WinFactorReduceFactory { by: *degregation_2 });
    (monte_limit, *c, TwoScoreReducerFactory::new(wri1, wri2).limiter_from(*el_threshold), None)
}

fn do_random_playoffs<G: MonteCarloGame + CheckWinMonteCarloGame + 'static>(monte_limit: MonteLimit, times: usize, vals: &[(RandomValues, AtomicU32)]) {
    let config_from_random_val = |rv| config_from_rv(monte_limit, rv);

    let game_count = AtomicU32::new(0);
    let total_game_count = (0..vals.len()).map(|i| i * times).sum::<usize>();

    vals.par_iter().enumerate()
        .flat_map(|(i, p1)| vals[..i].par_iter().map(move |p2| (p1, p2)))
        .for_each(|((rv1, wins1), (rv2, wins2))| {
            let config1 = config_from_random_val(rv1);
            let config2 = config_from_random_val(rv2);
            for i in 0..times {
                let mut players: [Box<dyn GamePlayer<G>>; 2] = [
                    Box::new(MonteCarloStrategyV7::strategy_of(config1.clone())),
                    Box::new(MonteCarloStrategyV7::strategy_of(config2.clone())),
                ];
                let switch = i % 2 != 0;
                if switch {
                    players.swap(0, 1)
                }
                let (winner, player) = run_game(players);
                if winner == Winner::TIE {
                    wins1.fetch_add(1, Ordering::Relaxed);
                    wins2.fetch_add(1, Ordering::Relaxed);
                } else {
                    let p1 = if !switch { TwoPlayer::P1 } else {TwoPlayer::P2 };
                    if player == p1 {
                        wins1.fetch_add(2, Ordering::Relaxed);
                    } else {
                        wins2.fetch_add(2, Ordering::Relaxed);
                    }
                }

                let played_games = game_count.fetch_add(1, Ordering::AcqRel);
                if played_games % 32 < 8 || played_games as usize == total_game_count {
                    print!("\rgame_count: {} of {total_game_count}", played_games);
                    if played_games as usize == total_game_count {
                        println!()
                    }
                }
            }
    })
}

fn run_game<G: MonteCarloGame + 'static>(mut config: [Box<dyn GamePlayer<G>>; 2]) -> (Winner, TwoPlayer) {
    let mut game = G::new();
    let mut last_move = None;
    loop {
        let config = match game.player() {
            TwoPlayer::P1 => &mut config[0],
            TwoPlayer::P2 => &mut config[1],
        };
        let m = config.make_move(&game, last_move);
        let (new_game, winner) = game.make_move(&m)
            .expect("could not make move");
        game = new_game;
        last_move = Some(m);
        if let Some(winner) = winner {
            break (winner, game.player());
        }
    }
}