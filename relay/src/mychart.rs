extern crate iced;
extern crate plotters;

use crate::{Message, State};
use iced::{
    widget::{Column, Container, Text},
    window, Alignment, Element, Length,
};
use plotters::{coord::Shift, prelude::*};
use plotters_backend::DrawingBackend;
use plotters_iced::{plotters_backend, Chart, ChartWidget, DrawingArea};

#[allow(unused)]
#[derive(Default)]
pub(crate) struct MyChart {
    pub points: Vec<(f32, f32)>,
}

impl MyChart {
    pub fn view(&self) -> Element<Message> {
        //might need to return a message element? not sure where
        /*  Column::new()
        .push(ChartWidget::new(self))
        .into() */
        ChartWidget::new(self).into()
    }

    pub fn new() -> Self {
        Self { points: Vec::new() }
    }
}

impl Chart<Message> for MyChart {
    type State = Vec<(f32, f32)>;

    fn build_chart<DB: DrawingBackend>(&self, _state: &Self::State, _builder: ChartBuilder<DB>) {}

    fn draw_chart<DB: DrawingBackend>(&self, state: &Self::State, root: DrawingArea<DB, Shift>) {
        let mut chart = ChartBuilder::on(&root)
            .margin(30)
            .caption("Live Chart", ("sans-serif", 22))
            .build_cartesian_2d(-1f32..100f32, -1.5f32..1.5f32)
            .unwrap();

        chart.configure_mesh().draw().unwrap();

        chart
            /* .draw_series(LineSeries::new(state.clone(), &RED))
            .unwrap(); */
            .draw_series(
                LineSeries::new(
                    //self.points.iter().map(|x| (x.0, x.1)),
                    self.points.iter().cloned(),
                    &RED,
                ), //.border_style(ShapeStyle::from(RGBColor = RGBColor(0, 175, 255)).stroke_width(2)),
            )
            .expect("failed to draw chart data");
    }
}
