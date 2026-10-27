//! Times the damage pass on the device: every shape of a frame updating the tile hashes.
//! Run with `just bench`.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use esp_hal::clock::CpuClock;
    use esp_hal::time::Instant;
    use yonder_graphics::damage::DamageGrid;
    use yonder_graphics::prelude::*;

    /// A 390×390 screen in 16 px tiles.
    type Grid = DamageGrid<Rgb565, 25, 25>;

    const FRAMES: i16 = 100;

    #[init]
    fn init() {
        let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
        let peripherals = esp_hal::init(config);

        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        let sw_interrupt =
            esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
        esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

        rtt_target::rtt_init_defmt!();
    }

    /// A background, `count` widgets on a grid (buttons, dots and rings), and a dot at
    /// `moving_x`.
    fn scene(grid: &mut Grid, count: usize, moving_x: i16) {
        let button = Style::new()
            .fill(Paint::solid(Rgb565::BLUE))
            .stroke(Stroke::new(2, Paint::solid(Rgb565::WHITE)));
        let dot = Style::new().fill(Paint::solid(Rgb565::YELLOW));
        let ring = Style::new().stroke(Stroke::new(3, Paint::solid(Rgb565::GREEN)));
        Rectangle::new(Point::new(0, 0), Point::new(390, 390))
            .with_style(Style::new().fill(Paint::solid(Rgb565::BLACK)))
            .draw(grid);
        let per_row = (1..).find(|k| k * k >= count).unwrap();
        let cell = 390 / per_row as i16;
        for i in 0..count {
            let (x, y) = ((i % per_row) as i16 * cell, (i / per_row) as i16 * cell);
            let (top_left, bottom_right) =
                (Point::new(x + 2, y + 2), Point::new(x + cell - 2, y + cell - 2));
            match i % 3 {
                0 => Rectangle::new(top_left, bottom_right)
                    .with_style(button)
                    .draw(grid),
                1 => Circle::in_box(BoundingBox::new(top_left, bottom_right))
                    .with_style(dot)
                    .draw(grid),
                _ => Circle::in_box(BoundingBox::new(top_left, bottom_right))
                    .with_style(ring)
                    .draw(grid),
            }
        }
        Circle::at_pixel_center(Point::new(moving_x, 195), 10)
            .with_style(dot)
            .draw(grid);
    }

    #[test]
    fn damage_pass() {
        for count in [10, 50, 200] {
            let mut grid = Grid::new();
            let start = Instant::now();
            for frame in 0..FRAMES {
                scene(&mut grid, count, 100 + frame);
                grid.next_frame();
            }
            let micros = start.elapsed().as_micros();
            // One more frame, to count what moving the dot damaged.
            scene(&mut grid, count, 100 + FRAMES);
            let damaged = (0..25)
                .flat_map(|row| (0..25).map(move |column| (column, row)))
                .filter(|&(column, row)| grid.is_damaged(column, row))
                .count();
            defmt::info!(
                "damage pass, {=usize} widgets: {=u64} ns per frame, {=usize} damaged tiles",
                count,
                micros * 1000 / FRAMES as u64,
                damaged
            );
        }
    }
}
