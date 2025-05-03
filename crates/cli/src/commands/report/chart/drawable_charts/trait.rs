use plotters::{
    coord::Shift,
    drawing::IntoDrawingArea,
    prelude::{BitMapBackend, DrawingArea},
    style::RGBColor,
};
use tracing::info;

pub trait DrawableChart {
    fn define_chart(
        &self,
        root: &DrawingArea<BitMapBackend, Shift>,
    ) -> Result<(), Box<dyn std::error::Error>>;

    fn draw(&self, filepath: &str) -> Result<(), Box<dyn std::error::Error>> {
        let root = BitMapBackend::new(filepath, (1024, 768)).into_drawing_area();
        root.fill(&RGBColor(255, 255, 255))
            .expect("invalid fill color");
        self.define_chart(&root)?;
        root.present()?;
        info!("saved chart to {filepath}");

        Ok(())
    }
}
