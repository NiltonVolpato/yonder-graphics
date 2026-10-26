impl Rasterizable for Rectangle {
    fn rasterize(&self, clip: BoundingBox) -> impl Iterator<Item = Coverage> {
        let bounds = self.bounds();
        let intersection = BoundingBox::new(
            Point::new(
                bounds.top_left.x.max(clip.top_left.x),
                bounds.top_left.y.max(clip.top_left.y),
            ),
            Point::new(
                bounds.bottom_right.x.min(clip.bottom_right.x),
                bounds.bottom_right.y.min(clip.bottom_right.y),
            ),
        );

        if intersection.width() <= 0 || intersection.height() <= 0 {
            None
        } else {
            Some(Coverage::Interior(intersection))
        }
        .into_iter()
    }
}
