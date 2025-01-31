use contender_core::db::RunTx;
use plotters::{
    backend::BitMapBackend,
    chart::ChartBuilder,
    drawing::IntoDrawingArea,
    series::Histogram,
    style::{
        full_palette::{BLUE, WHITE},
        Color,
    },
};

pub struct TimeToInclusionChart {
    /// Maps number of times a block was included in a time period.
    inclusion_times: Vec<u64>,
}

impl TimeToInclusionChart {
    fn new() -> Self {
        Self {
            inclusion_times: Default::default(),
        }
    }

    pub fn build(run_txs: &[RunTx]) -> Self {
        let mut chart = TimeToInclusionChart::new();

        for tx in run_txs {
            let tti = tx.end_timestamp - tx.start_timestamp;
            chart.add_inclusion_time(tti as u64);
        }

        chart
    }

    fn add_inclusion_time(&mut self, time_to_include: u64) {
        self.inclusion_times.push(time_to_include);
    }

    pub fn draw(&self, filepath: impl AsRef<str>) -> Result<(), Box<dyn std::error::Error>> {
        let root = BitMapBackend::new(filepath.as_ref(), (1024, 768)).into_drawing_area();
        root.fill(&WHITE)?;

        let min_tti = self
            .inclusion_times
            .iter()
            .min()
            .expect("no time-to-inclusion data found");
        let max_tti = self
            .inclusion_times
            .iter()
            .max()
            .expect("no time-to-inclusion data found");

        let mut chart = ChartBuilder::on(&root)
            .margin(15)
            .x_label_area_size(60)
            .y_label_area_size(40)
            .build_cartesian_2d(*min_tti..*max_tti + 1, 0..self.inclusion_times.len() as u32)?;

        chart
            .configure_mesh()
            .label_style(("sans-serif", 15))
            .x_label_offset(10)
            .x_desc("Time to Inclusion (seconds)")
            .y_desc("# Transactions")
            .draw()?;

        chart.draw_series(
            Histogram::vertical(&chart)
                .style(BLUE.filled())
                .data(self.inclusion_times.iter().map(|&x| (x, 1))),
        )?;

        root.present()?;

        println!("saved chart to {}", filepath.as_ref());
        Ok(())
    }
}
