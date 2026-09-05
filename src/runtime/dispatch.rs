/// Guest-to-native dispatch state.
#[derive(Debug, Default)]
pub struct Dispatcher {
    pub blocks_executed: u64,
}

impl Dispatcher {
    #[inline]
    pub fn tick(&mut self) {
        self.blocks_executed = self.blocks_executed.wrapping_add(1);
    }
}
