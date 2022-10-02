const base: f64 = 2.;

const min_tick: i32 = -887272;

pub fn get_min_tick() -> i32 {
  min_tick
}
pub fn get_max_tick() -> i32 {
  -min_tick
}
pub fn get_q96() -> f64 {
  base.powf(96.)
}
