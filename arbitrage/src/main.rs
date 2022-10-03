#![allow(dead_code)]
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::thread;
use std::time::Duration;

mod math;

#[derive(PartialEq, Copy, Clone)]
enum Token {
    Eth,
    Dai,
}

fn price_to_tick(price: f64) -> f64 {
    price.log(1.001).floor()
}

fn tick_to_price(tick: i32) -> f64 {
    let base: f64 = 1.001;
    let num: f64 = base.powi(tick);
    num.sqrt() * math::get_q96()
}

fn price_to_sqrtp(price: f64) -> f64 {
    price.sqrt() * math::get_q96()
}

fn liquidity0(amount: f64, pa: f64, pb: f64) -> f64 {
    let q96 = math::get_q96();
    if pa > pb {
        return (amount * (pa * pb) / q96) / (pb - pa);
    } else {
        return (amount * (pb * pa) / q96) / (pa - pb);
    }
}

fn liquidity1(amount: f64, pa: f64, pb: f64) -> f64 {
    let q96 = math::get_q96();
    if pa > pb {
        return amount * q96 / (pb - pa);
    } else {
        return amount * q96 / (pa - pb);
    }
}

fn calc_amount0(liq: f64, lower_tick: f64, upper_tick: f64) -> f64 {
    let q96 = math::get_q96();
    if upper_tick > lower_tick {
        return liq * q96 * (upper_tick - lower_tick) / lower_tick / upper_tick;
    } else {
        return liq * q96 * (lower_tick - upper_tick) / upper_tick / lower_tick;
    }
}

fn calc_amount1(liq: f64, lower_tick: f64, upper_tick: f64) -> f64 {
    let q96 = math::get_q96();
    if upper_tick > lower_tick {
        return liq * (upper_tick - lower_tick) / q96;
    } else {
        return liq * (lower_tick - upper_tick) / q96;
    }
}

fn calc_price_diff(amount_in: f64, liquidity: f64) -> f64 {
    (amount_in * math::get_q96()) / liquidity
}

fn get_next_sqrt_price_from_input(
    sqrt_price_current_x96: f64,
    liquidity: f64,
    amount_remaining: f64,
    zero_for_one: bool,
) -> f64 {
    let q96 = math::get_q96();
    if zero_for_one {
        return (liquidity * q96 * sqrt_price_current_x96)
            / (liquidity * q96 + amount_remaining * sqrt_price_current_x96);
    } else {
        return sqrt_price_current_x96 + (amount_remaining * q96) / liquidity;
    }
}

fn compute_swap_step(
    sqrt_price_current_x96: f64,
    sqrt_price_target_x96: f64,
    liquidity: f64,
    amount_remaining: f64,
) -> (f64, f64, f64) {
    let zero_for_one = sqrt_price_current_x96 >= sqrt_price_target_x96;

    let amount_in_pre_calc = if zero_for_one {
        calc_amount0(liquidity, sqrt_price_current_x96, sqrt_price_target_x96)
    } else {
        calc_amount1(liquidity, sqrt_price_current_x96, sqrt_price_target_x96)
    };

    let sqrt_price_next_x96: f64;

    if amount_remaining >= amount_in_pre_calc {
        sqrt_price_next_x96 = sqrt_price_target_x96;
    } else {
        sqrt_price_next_x96 = get_next_sqrt_price_from_input(
            sqrt_price_current_x96,
            liquidity,
            amount_remaining,
            zero_for_one,
        );
    }

    let amount_in = calc_amount0(liquidity, sqrt_price_current_x96, sqrt_price_next_x96);

    let amount_out = calc_amount1(liquidity, sqrt_price_current_x96, sqrt_price_next_x96);

    if zero_for_one {
        (sqrt_price_next_x96, amount_in, amount_out)
    } else {
        (sqrt_price_next_x96, amount_out, amount_in)
    }
}

struct Tick {
    liquidity: RwLock<f64>,
    initialized: RwLock<bool>,
}

struct Position {
    liquidity: RwLock<f64>,
}

struct uniswap_v3_pool {
    token_0: Token,
    token_1: Token,
    min_tick: i32,
    max_tick: i32,
    balance_0: RwLock<f64>,
    balance_1: RwLock<f64>,
    tick_mapping: RwLock<HashMap<i32, Tick>>,
    liquidity_mapping: RwLock<HashMap<i32, f64>>,
    position_mapping: RwLock<HashMap<i32, Position>>,
    sqrt_price_x96: RwLock<f64>,
    tick: RwLock<i32>,
    liquidity: RwLock<f64>,
}

impl uniswap_v3_pool {
    fn update(&mut self, tick: i32, liquidity_delta: f64) -> bool {
        let default_tick = Tick {
            liquidity: RwLock::new(0.),
            initialized: RwLock::new(false),
        };
        let tick_map = &mut self.tick_mapping.write().unwrap();

        let info = tick_map.entry(tick).or_insert(default_tick);

        let liquidity_before = *info.liquidity.read().unwrap();

        let liquidity_after = liquidity_before + liquidity_delta;

        if liquidity_before == 0. {
            *info.initialized.write().unwrap() = true;
            self.liquidity_mapping
                .write()
                .unwrap()
                .insert(tick, liquidity_after);
        }

        *info.liquidity.write().unwrap() = liquidity_after;

        let flipped = (liquidity_after == 0.) != (liquidity_before == 0.);

        flipped
    }

    fn _update_position(
        &mut self,
        owner: &Trader,
        lower_tick: i32,
        upper_tick: i32,
        liquidity_delta: f64,
    ) {
        let flipped_lower = self.update(lower_tick, liquidity_delta);
        let flipped_upper = self.update(upper_tick, liquidity_delta);

        if flipped_lower {
            self.liquidity_mapping
                .write()
                .unwrap()
                .insert(lower_tick, 1.);
        }
        if flipped_upper {
            self.liquidity_mapping
                .write()
                .unwrap()
                .insert(upper_tick, 1.);
        }

        let default_position = Position {
            liquidity: RwLock::new(0.),
        };

        let position_map = &mut self.position_mapping.write().unwrap();

        let position = position_map.entry(owner.id).or_insert(default_position);

        *position.liquidity.write().unwrap() += liquidity_delta;

        if liquidity_delta < 0. {
            if flipped_lower {
                self.liquidity_mapping.write().unwrap().remove(&lower_tick);
            }
            if flipped_upper {
                self.liquidity_mapping.write().unwrap().remove(&upper_tick);
            }
        }
    }

    fn _modify_position(
        &mut self,
        owner: &Trader,
        lower_tick: i32,
        upper_tick: i32,
        liquidity_delta: f64,
    ) -> (f64, f64) {
        let mut amount0: f64 = 0.;
        let mut amount1: f64 = 0.;
        let sqrt_price_x96 = *self.sqrt_price_x96.read().unwrap();
        let tick = *self.tick.read().unwrap();
        self._update_position(owner, lower_tick, upper_tick, liquidity_delta);
        if liquidity_delta != 0. {
            if tick < lower_tick {
                amount0 = calc_amount0(
                    liquidity_delta,
                    tick_to_price(lower_tick),
                    tick_to_price(upper_tick),
                );
            } else if tick < upper_tick {
                amount0 = calc_amount0(liquidity_delta, sqrt_price_x96, tick_to_price(upper_tick));

                amount1 = calc_amount1(liquidity_delta, tick_to_price(lower_tick), sqrt_price_x96);
                *self.liquidity.write().unwrap() += liquidity_delta;
            } else {
                amount1 = calc_amount1(
                    liquidity_delta,
                    tick_to_price(lower_tick),
                    tick_to_price(upper_tick),
                );
            }
        }

        (amount0, amount1)
    }

    fn mint(&mut self, owner: &Trader, lower_tick: i32, upper_tick: i32, liquidity_delta: f64) {
        if !(lower_tick >= upper_tick || lower_tick < self.min_tick || upper_tick > self.max_tick)
            && liquidity_delta != 0.
        {
            let (amount0, amount1) =
                self._modify_position(owner, lower_tick, upper_tick, liquidity_delta);
            if amount0 > 0. {
                *self.balance_0.write().unwrap() += amount0
            }
            if amount1 > 0. {
                *self.balance_1.write().unwrap() += amount1
            }

            if self.token_0 == Token::Eth {
                *owner.amt_eth.write().unwrap() -= amount0;
                *owner.amt_dai.write().unwrap() -= amount1;
            } else {
                *owner.amt_eth.write().unwrap() -= amount1;
                *owner.amt_dai.write().unwrap() -= amount0;
            }
        }
    }
}
struct SwapState {
    amount_specified_remaining: f64,
    amount_calculated: f64,
    sqrt_price_x96: f64,
    tick: i32,
    liquidity: f64,
}

struct StepState {
    sqrt_price_start_x96: f64,
    next_tick: i32,
    sqrt_price_next_x96: f64,
    amount_in: f64,
    amount_out: f64,
}

// [next_initialized_tick] returns -1 if there is no tick available in the provided direction of liquidity. Returns the tick with liquidity if one is found.
fn next_initialized_tick(liquidity_mapping: HashMap<i32, f64>, tick: i32, is_up: bool) -> i32 {
    let liquidity_map = liquidity_mapping;

    let mut sorted_keys: Vec<i32> = liquidity_map.into_keys().collect();
    sorted_keys.sort_unstable();
    let start_index: i32;
    if is_up {
        start_index = match sorted_keys.iter().position(|&x| x >= tick) {
            None => -1,
            Some(x) => x as i32,
        };
    } else {
        start_index = match sorted_keys.iter().position(|&x| x <= tick) {
            None => -1,
            Some(x) => x as i32,
        };
    }
    match sorted_keys.get(start_index as usize) {
        None => -1,
        x => *x.unwrap(),
    }
}

fn cross(tick_mapping: &HashMap<i32, Tick>, next_tick: i32) -> f64 {
    let tick = tick_mapping.get(&next_tick).unwrap();
    *tick.liquidity.read().unwrap()
}

fn v3_swap(
    trader: &mut Trader,
    pool: &uniswap_v3_pool,
    token_in: Token,
    amount_specified: f64,
    fee: f64,
) {
    let zero_for_one: bool = token_in == pool.token_0;

    let mut state = SwapState {
        amount_specified_remaining: amount_specified,
        amount_calculated: 0.,
        sqrt_price_x96: *pool.sqrt_price_x96.read().unwrap(),
        tick: pool.tick.read().unwrap().clone(),
        liquidity: *pool.liquidity.read().unwrap(),
    };

    while state.amount_specified_remaining > 0. {
        let next_tick = next_initialized_tick(
            pool.liquidity_mapping.read().unwrap().clone(),
            state.tick,
            zero_for_one,
        );
        let sqrt_price_next_x96 = tick_to_price(next_tick);

        let (next_sqrt_price_x96, amount_in, amount_out) = compute_swap_step(
            state.sqrt_price_x96,
            sqrt_price_next_x96,
            state.liquidity,
            state.amount_specified_remaining,
        );

        let step = StepState {
            sqrt_price_start_x96: state.sqrt_price_x96,
            next_tick: next_tick,
            sqrt_price_next_x96: next_sqrt_price_x96,
            amount_in: amount_in,
            amount_out: amount_out,
        };

        if step.amount_in == 0. {
            return;
        }

        state.sqrt_price_x96 = next_sqrt_price_x96;
        state.amount_specified_remaining -= step.amount_in;
        state.amount_calculated += step.amount_out;

        if state.sqrt_price_x96 == step.sqrt_price_next_x96 {
            let mut liquidity_delta = cross(&*pool.tick_mapping.read().unwrap(), step.next_tick);

            if zero_for_one {
                liquidity_delta = -liquidity_delta;
            }

            state.liquidity += liquidity_delta;

            state.tick = step.next_tick;
        } else {
            state.tick = price_to_tick(state.sqrt_price_x96) as i32;
        }
        if *pool.liquidity.write().unwrap() != state.liquidity {
            *pool.liquidity.write().unwrap() = state.liquidity
        }
    }

    let mut pooltick = pool.tick.write().unwrap();
    if state.tick != *pooltick {
        *pooltick = state.tick;
        *pool.sqrt_price_x96.write().unwrap() = state.sqrt_price_x96
    }
    let (amount0, amount1) = if zero_for_one {
        (
            amount_specified - state.amount_specified_remaining,
            state.amount_calculated,
        )
    } else {
        (
            state.amount_calculated,
            amount_specified - state.amount_specified_remaining,
        )
    };

    if zero_for_one {
        *pool.balance_0.write().unwrap() += amount0;
        *pool.balance_1.write().unwrap() -= amount1;
    } else {
        *pool.balance_0.write().unwrap() -= amount0;
        *pool.balance_1.write().unwrap() += amount1;
    }
    if token_in == Token::Eth {
        *trader.amt_eth.write().unwrap() -= amount0;
        *trader.amt_dai.write().unwrap() += (1. - fee) * amount1;
    } else {
        *trader.amt_dai.write().unwrap() -= amount1;
        *trader.amt_eth.write().unwrap() += (1. - fee) * amount0;
    }
}

struct Trader {
    id: i32,
    amt_eth: RwLock<f64>,
    amt_dai: RwLock<f64>,
}

fn calc_two_pool_arb_profit(
    x_in: f64,
    pool1: &uniswap_v3_pool,
    pool2: &uniswap_v3_pool,
    token_in: Token,
) -> f64 {
    let pooll1_copy = pool1.clone();
    let pool2_copy = pool2.clone();
    let mut example_trader = Trader {
        id: 1,
        amt_dai: RwLock::new(100.),
        amt_eth: RwLock::new(10000000000000.),
    };

    let start_dai = 100.;
    let start_eth = 10000000000000.;

    if token_in == Token::Eth {
        v3_swap(&mut example_trader, &pooll1_copy, Token::Eth, x_in, 0.03);

        let change = *example_trader.amt_dai.read().unwrap() - start_dai;

        v3_swap(&mut example_trader, &pool2_copy, Token::Dai, change, 0.03);

        let profit = *example_trader.amt_eth.read().unwrap() - start_eth;

        return profit;
    } else {
        v3_swap(&mut example_trader, &pooll1_copy, Token::Dai, x_in, 0.03);

        let change = *example_trader.amt_eth.read().unwrap() - start_eth;

        v3_swap(&mut example_trader, &pool2_copy, Token::Eth, change, 0.03);

        let profit = *example_trader.amt_dai.read().unwrap() - start_dai;

        return profit;
    }
}

fn find_optimal_arb(
    pool1: &uniswap_v3_pool,
    pool2: &uniswap_v3_pool,
    token_in: Token,
    max_amt_in: f64,
) -> f64 {
    let mut amt = 1.;
    let mut max_out = f64::MIN;
    let mut opt_amt = 0.;
    while amt <= max_amt_in {
        let amt_out = calc_two_pool_arb_profit(amt, pool1, pool2, token_in);
        if amt_out > max_out {
            max_out = amt_out;
            opt_amt = amt;
        }
        amt += 100.;
    }
    opt_amt
}

fn main() {
    let trader = Trader {
        id: 2,
        amt_eth: RwLock::new(2000.),
        amt_dai: RwLock::new(10000.),
    };
    let mut pool1 = uniswap_v3_pool {
        liquidity: RwLock::new(0.),
        max_tick: math::get_max_tick(),
        min_tick: math::get_min_tick(),
        position_mapping: RwLock::new(HashMap::new()),
        tick_mapping: RwLock::new(HashMap::new()),
        liquidity_mapping: RwLock::new(HashMap::new()),
        sqrt_price_x96: RwLock::new(5602277097478614198912276234240.),
        tick: RwLock::new(85176),
        token_0: Token::Eth,
        token_1: Token::Dai,
        balance_0: RwLock::new(0.),
        balance_1: RwLock::new(0.),
    };

    let mut pool2 = uniswap_v3_pool {
        liquidity: RwLock::new(0.),
        max_tick: math::get_max_tick(),
        min_tick: math::get_min_tick(),
        position_mapping: RwLock::new(HashMap::new()),
        tick_mapping: RwLock::new(HashMap::new()),
        liquidity_mapping: RwLock::new(HashMap::new()),
        sqrt_price_x96: RwLock::new(5602277097478614198912276234240.),
        tick: RwLock::new(85176),
        token_0: Token::Eth,
        token_1: Token::Dai,
        balance_0: RwLock::new(0.),
        balance_1: RwLock::new(0.),
    };

    pool1.mint(&trader, -86000, 86000, 100000000000000.);
    pool2.mint(&trader, -86000, 86000, 1000000000000000000.);

    let safepool1 = Arc::new(RwLock::new(pool1));
    let safepool2 = Arc::new(RwLock::new(pool2));

    let viewpool1 = Arc::clone(&safepool1);
    let viewpool2 = Arc::clone(&safepool2);

    let mut handles = vec![];

    let writer = thread::spawn(move || {
        for _ in 0..20 {
            let mut rng = rand::thread_rng();
            let randomness = rng.gen_range(0..10);

            if randomness > 5 {
                *&Arc::clone(&safepool1)
                    .write()
                    .unwrap()
                    .mint(&trader, -86000, 86000, 20000.);
                *&Arc::clone(&safepool2)
                    .write()
                    .unwrap()
                    .mint(&trader, -86000, 86000, 20000.);
            } else {
                *&Arc::clone(&safepool1)
                    .write()
                    .unwrap()
                    .mint(&trader, -86000, 86000, -10000.);
                *&Arc::clone(&safepool2)
                    .write()
                    .unwrap()
                    .mint(&trader, -86000, 86000, -10000.);
            }
        }
        thread::sleep(Duration::from_millis(1000));
    });

    let searcher = thread::spawn(move || {
        for _ in 0..10 {
            let b1 = find_optimal_arb(
                &Arc::clone(&viewpool1).read().unwrap(),
                &Arc::clone(&viewpool2).read().unwrap(),
                Token::Eth,
                1000000.0,
            );
            let b2 = find_optimal_arb(
                &Arc::clone(&viewpool2).read().unwrap(),
                &Arc::clone(&viewpool1).read().unwrap(),
                Token::Eth,
                1000000.0,
            );

            println!(
                "Profit from sending {:?}, {:?}",
                b1,
                calc_two_pool_arb_profit(
                    b1,
                    &Arc::clone(&viewpool1).read().unwrap(),
                    &Arc::clone(&viewpool2).read().unwrap(),
                    Token::Eth,
                )
            );
            println!(
                "Profit from sending {:?}, {:?}",
                b2,
                calc_two_pool_arb_profit(
                    b1,
                    &Arc::clone(&viewpool2).read().unwrap(),
                    &Arc::clone(&viewpool1).read().unwrap(),
                    Token::Eth
                )
            );
            thread::sleep(Duration::from_millis(2000));
        }
    });
    handles.push(writer);
    handles.push(searcher);
    for i in handles {
        i.join().unwrap();
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn set_up_pool(
        mint: bool,
        lower_tick: i32,
        upper_tick: i32,
        liquidity: f64,
    ) -> (Trader, uniswap_v3_pool) {
        let trader = Trader {
            id: 2,
            amt_eth: RwLock::new(10000000000.),
            amt_dai: RwLock::new(10000000000.),
        };
        let mut pool = uniswap_v3_pool {
            liquidity: RwLock::new(0.),
            max_tick: math::get_max_tick(),
            min_tick: math::get_min_tick(),
            position_mapping: RwLock::new(HashMap::new()),
            tick_mapping: RwLock::new(HashMap::new()),
            liquidity_mapping: RwLock::new(HashMap::new()),
            sqrt_price_x96: RwLock::new(5602277097478614198912276234240.),
            tick: RwLock::new(85176),
            token_0: Token::Eth,
            token_1: Token::Dai,
            balance_0: RwLock::new(0.),
            balance_1: RwLock::new(0.),
        };
        if mint {
            pool.mint(&trader, lower_tick, upper_tick, liquidity);
        }

        (trader, pool)
    }

    #[test]
    fn price_to_sqrt_price() {
        assert_eq!(price_to_sqrtp(5000.), 5.602277097478614e30);
    }

    #[test]
    fn v3_test_mint() {
        let trader = Trader {
            id: 2,
            amt_eth: RwLock::new(2000.),
            amt_dai: RwLock::new(10000.),
        };
        let mut pool = uniswap_v3_pool {
            liquidity: RwLock::new(0.),
            max_tick: math::get_max_tick(),
            min_tick: math::get_min_tick(),
            position_mapping: RwLock::new(HashMap::new()),
            tick_mapping: RwLock::new(HashMap::new()),
            liquidity_mapping: RwLock::new(HashMap::new()),
            sqrt_price_x96: RwLock::new(5602277097478614198912276234240.),
            tick: RwLock::new(85176),
            token_0: Token::Eth,
            token_1: Token::Dai,
            balance_0: RwLock::new(0.),
            balance_1: RwLock::new(0.),
        };

        pool.mint(&trader, 84222, 86129, 1517882343751509868544.);

        assert_eq!(
            *pool.sqrt_price_x96.read().unwrap(),
            5602277097478614198912276234240.0
        );
    }
    #[test]
    fn v3_test_remove() {
        let trader = Trader {
            id: 2,
            amt_eth: RwLock::new(2000.),
            amt_dai: RwLock::new(10000.),
        };
        let mut pool = uniswap_v3_pool {
            liquidity: RwLock::new(0.),
            max_tick: math::get_max_tick(),
            min_tick: math::get_min_tick(),
            position_mapping: RwLock::new(HashMap::new()),
            tick_mapping: RwLock::new(HashMap::new()),
            liquidity_mapping: RwLock::new(HashMap::new()),
            sqrt_price_x96: RwLock::new(5602277097478614198912276234240.),
            tick: RwLock::new(85176),
            token_0: Token::Eth,
            token_1: Token::Dai,
            balance_0: RwLock::new(0.),
            balance_1: RwLock::new(0.),
        };

        pool.mint(&trader, 84222, 86129, 1517882343751509868544.);

        let liq = *pool.liquidity.read().unwrap();

        assert_eq!(liq, 1517882343751509868544.);

        pool.mint(&trader, 84222, 86129, -1517882343751509868544.);

        assert_eq!(
            *pool.sqrt_price_x96.read().unwrap(),
            5602277097478614198912276234240.0
        );
        let new_liquidity = *pool.liquidity.read().unwrap();
        assert_eq!(new_liquidity, 0.)
    }

    #[test]
    fn test_swap_eth() {
        let (mut trader, pool) = set_up_pool(true, -86000, 86000, 100000000000.);
        let original = *trader.amt_eth.read().unwrap();
        let og_dai = *trader.amt_dai.read().unwrap();

        v3_swap(&mut trader, &pool, Token::Eth, 1000000., 0.03);

        let post = *trader.amt_eth.read().unwrap();
        let post_dai = *trader.amt_dai.read().unwrap();

        assert_eq!(original > post, true);
        assert_eq!(post_dai > og_dai, true);
    }

    #[test]
    fn test_swap_dai() {
        let (mut trader, pool) = set_up_pool(true, -86000, 86000, 10000000000000.);
        let original = *trader.amt_eth.read().unwrap();
        let og_dai = *trader.amt_dai.read().unwrap();

        v3_swap(&mut trader, &pool, Token::Dai, 100., 0.03);

        let post = *trader.amt_eth.read().unwrap();
        let post_dai = *trader.amt_dai.read().unwrap();

        assert_eq!(original < post, true);
        assert_eq!(post_dai < og_dai, true);
    }

    #[test]
    fn benchmark_search_for_arb() {
        main()
    }
}
