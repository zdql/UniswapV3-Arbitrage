#![allow(dead_code)]
use rand::Rng;
use std::sync::Arc;
use std::sync::RwLock;
use std::thread;
use std::time::Duration;

#[derive(PartialEq, Copy, Clone)]
enum Token {
    Eth,
    Dai,
}

struct Pool {
    token_x: Token,
    token_y: Token,
    x: RwLock<f64>,
    y: RwLock<f64>,
    k: RwLock<f64>,
}

struct Trader {
    amt_eth: RwLock<f64>,
    amt_dai: RwLock<f64>,
}

fn add(pool: &Pool, add_to_x: f64, add_to_y: f64) {
    *pool.x.write().unwrap() += add_to_x;
    *pool.y.write().unwrap() += add_to_y;
    *pool.k.write().unwrap() = *pool.x.read().unwrap() + *pool.y.read().unwrap();
}

fn remove(pool: &Pool, rem_from_x: f64, rem_from_y: f64) {
    *pool.x.write().unwrap() -= rem_from_x;
    *pool.y.write().unwrap() -= rem_from_y;
    *pool.k.write().unwrap() = *pool.x.read().unwrap() + *pool.y.read().unwrap();
}

fn get_amount_out(amount_in: f64, pool: &Pool, token_in: Token, fee: f64) -> f64 {
    let amount_in_less_fee = amount_in * (1. - fee);
    let py = *pool.y.read().unwrap();
    let px = *pool.x.read().unwrap();
    if token_in == pool.token_x {
        let price = py / px;
        let amount_out = amount_in_less_fee * price;
        if amount_out <= py {
            remove(pool, 0., amount_out);
            add(pool, amount_in, 0.);
            return amount_out;
        } else {
            return 0.;
        }
    } else {
        let price = px / py;
        let amount_out = amount_in_less_fee * price;
        if amount_out <= px {
            remove(pool, amount_out, 0.);
            add(pool, 0., amount_in);
            return amount_out;
        } else {
            return 0.;
        }
    }
}

fn swap(trader: &mut Trader, pool: &Pool, token_in: Token, amount_in: f64, fee: f64) {
    let amt_eth = *trader.amt_eth.read().unwrap();
    let amt_dai = *trader.amt_dai.read().unwrap();
    if token_in == Token::Eth && amt_eth > amount_in {
        let amt_out = get_amount_out(amount_in, pool, token_in, fee);
        if amt_out > 0. {
            *trader.amt_eth.write().unwrap() = amt_eth - amount_in;
            *trader.amt_dai.write().unwrap() = amt_dai + amt_out;
        }
    } else if token_in == Token::Dai && amt_dai > amount_in {
        let amt_out = get_amount_out(amount_in, pool, token_in, fee);
        if amt_out > 0. {
            *trader.amt_dai.write().unwrap() = amt_dai - amt_out;
            *trader.amt_eth.write().unwrap() = amt_dai + amount_in;
        }
    }
}

fn calc_two_pool_arb_profit(x_in: f64, xr1: f64, xr2: f64, yr1: f64, yr2: f64, fee: f64) -> f64 {
    let s = (fee * xr1 * x_in) / (yr1 + (fee * x_in));
    let n = fee * yr2 * s;
    let d = xr2 + fee * s;
    n / d
}

fn detect_arb(pool1: &Pool, pool2: &Pool, token_in: Token, fee: f64, amt_in: f64) -> f64 {
    let is_x_1 = token_in == pool1.token_x;
    let is_x_2 = token_in == pool2.token_x;

    let x1 = *pool1.x.read().unwrap();
    let x2 = *pool2.x.read().unwrap();
    let y1 = *pool1.y.read().unwrap();
    let y2 = *pool2.y.read().unwrap();

    let opt_amt = match (is_x_1, is_x_2) {
        (true, true) => calc_two_pool_arb_profit(amt_in, x1, x2, y1, y2, fee),
        (true, false) => calc_two_pool_arb_profit(amt_in, x1, y2, y1, x2, fee),
        (false, false) => calc_two_pool_arb_profit(amt_in, y1, y2, x1, x2, fee),
        (false, true) => calc_two_pool_arb_profit(amt_in, y1, x2, x1, y2, fee),
    };

    opt_amt
}

fn find_optimal_arb(pool1: &Pool, pool2: &Pool, token_in: Token, fee: f64, max_amt_in: f64) -> f64 {
    let mut amt = 0.01;
    let mut max_out = 0.;
    let mut opt_amt = 0.;
    while amt <= max_amt_in {
        let amt_out = detect_arb(pool1, pool2, token_in.clone(), fee, amt) - amt;
        if amt_out > max_out {
            max_out = amt_out;
            opt_amt = amt;
        }
        amt += 0.01;
    }
    opt_amt
}

fn main() {
    let pool1 = Arc::new(Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: RwLock::new(4.),
        y: RwLock::new(3500.),
        k: RwLock::new(3504.),
    });

    let pool2 = Arc::new(Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: RwLock::new(4.),
        y: RwLock::new(4000.),
        k: RwLock::new(4004.),
    });

    let safepool1 = Arc::clone(&pool1);
    let safepool2 = Arc::clone(&pool2);

    let mut handles = vec![];

    let writer = thread::spawn(move || {
        for _ in 1..20 {
            let mut rng = rand::thread_rng();
            let randomness = rng.gen_range(0..10);

            if randomness > 5 {
                add(&safepool1, 1., 2000.);
                add(&safepool2, 1., 1200.);
            } else {
                remove(&safepool1, 0.2, 500.);
                remove(&safepool2, 0.3, 600.);
            }
            thread::sleep(Duration::from_millis(1000));
        }
    });

    let searcher = thread::spawn(move || {
        for _ in 1..10 {
            let b1 = find_optimal_arb(
                &Arc::clone(&pool1),
                &Arc::clone(&pool2),
                Token::Eth,
                0.97,
                2.,
            );
            let b2 = find_optimal_arb(
                &Arc::clone(&pool2),
                &Arc::clone(&pool1),
                Token::Eth,
                0.97,
                2.,
            );
            println!(
                "Profit from sending {:?}, {:?}",
                b1,
                detect_arb(
                    &Arc::clone(&pool1),
                    &Arc::clone(&pool2),
                    Token::Eth,
                    0.97,
                    b1
                ) - b1
            );
            println!(
                "Profit from sending {:?}, {:?}",
                b2,
                detect_arb(
                    &Arc::clone(&pool2),
                    &Arc::clone(&pool1),
                    Token::Eth,
                    0.97,
                    b2,
                ) - b2
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

    #[test]
    fn initialize() {
        let xx = 1000.;
        let yy = 200.;
        let pool = Pool {
            token_x: Token::Eth,
            token_y: Token::Dai,
            x: RwLock::new(xx),
            y: RwLock::new(yy),
            k: RwLock::new(xx + yy),
        };
        let trader = Trader {
            amt_eth: RwLock::new(xx),
            amt_dai: RwLock::new(yy),
        };

        assert_eq!(*trader.amt_eth.read().unwrap(), 1000.);
        assert_eq!(*trader.amt_dai.read().unwrap(), 200.);

        assert_eq!(*pool.x.read().unwrap(), 1000.);
        assert_eq!(*pool.y.read().unwrap(), 200.);
    }

    #[test]
    fn add_and_remove() {
        let xx = 1000.;
        let yy = 200.;
        let pool = Arc::new(Pool {
            token_x: Token::Eth,
            token_y: Token::Dai,
            x: RwLock::new(xx),
            y: RwLock::new(yy),
            k: RwLock::new(xx + yy),
        });

        let safepool = Arc::clone(&pool);

        add(&safepool, 4., 4.);
        assert_eq!(*Arc::clone(&pool).x.read().unwrap(), 1004.);
        assert_eq!(*Arc::clone(&pool).y.read().unwrap(), 204.);
    }

    #[test]
    fn test_swap() {
        let xx = 1000.;
        let yy = 200.;
        let pool = Pool {
            token_x: Token::Eth,
            token_y: Token::Dai,
            x: RwLock::new(xx),
            y: RwLock::new(yy),
            k: RwLock::new(xx + yy),
        };
        let mut trader = Trader {
            amt_eth: RwLock::new(xx),
            amt_dai: RwLock::new(yy),
        };
        swap(&mut trader, &pool, Token::Eth, 1., 0.03);

        assert_eq!(*trader.amt_eth.read().unwrap(), 999.);
        assert_eq!(*trader.amt_dai.read().unwrap(), 200.194);
    }

    #[test]

    fn find_optimal_amount() {
        let pool1 = Arc::new(Pool {
            token_x: Token::Eth,
            token_y: Token::Dai,
            x: RwLock::new(4.),
            y: RwLock::new(3500.),
            k: RwLock::new(3504.),
        });
        let pool2 = Arc::new(Pool {
            token_x: Token::Eth,
            token_y: Token::Dai,
            x: RwLock::new(4.),
            y: RwLock::new(4000.),
            k: RwLock::new(4004.),
        });

        let b1 = find_optimal_arb(
            &Arc::clone(&pool1),
            &Arc::clone(&pool2),
            Token::Eth,
            0.97,
            2.,
        );
        let b2 = find_optimal_arb(
            &Arc::clone(&pool2),
            &Arc::clone(&pool1),
            Token::Eth,
            0.97,
            2.,
        );
        assert_eq!(b1, 1.9900000000000015);
        assert_eq!(
            detect_arb(
                &Arc::clone(&pool1),
                &Arc::clone(&pool2),
                Token::Eth,
                0.97,
                b1
            ) - b1,
            0.14755301325556314
        );
        assert_eq!(b2, 0.);
        assert_eq!(
            detect_arb(
                &Arc::clone(&pool1),
                &Arc::clone(&pool2),
                Token::Eth,
                0.97,
                b2
            ) - b2,
            0.
        );
    }

    #[test]
    fn benchmark_non_blocking_calculation() {
        main()
    }
}
