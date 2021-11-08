#[derive(Debug, Clone)]
pub struct OwnedEntity<E: Copy> {
    pub confirmed: E,
    pub predicted: E,
}

impl<E: Copy> OwnedEntity<E> {
    pub fn new(confirmed_entity: &E, predicted_entity: &E) -> Self {
        return Self {
            confirmed: *confirmed_entity,
            predicted: *predicted_entity,
        };
    }
}
