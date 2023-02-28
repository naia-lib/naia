use naia_bevy_shared::{EntityProperty, Message};

#[derive(Message)]
pub struct EntityAssignment {
    pub entity: EntityProperty,
    pub assign: bool,
    pub big_thing: String,
}

impl EntityAssignment {
    pub fn new(assign: bool) -> Self {
        Self {
            assign,
            entity: EntityProperty::new_empty(),
            big_thing: "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Donec sed justo a mi ultricies ultrices. \
            Sed porta, odio eu rhoncus venenatis, massa elit posuere nisl, sit amet venenatis mi mi at erat. \
            Vivamus non ullamcorper augue, non pharetra augue. Fusce erat ante, iaculis id maximus eu, faucibus consequat nisl. \
            Integer consequat consequat bibendum. Cras nisl est, lacinia nec ipsum vitae, pulvinar elementum orci. \
            Nullam in mauris vel nulla convallis laoreet vitae in turpis. Proin dignissim mattis ante, a lacinia neque sodales id. \
            Donec ac sollicitudin nunc. Nam luctus nulla ut nisi tristique, quis scelerisque neque elementum. Etiam a quam turpis. \
            Vestibulum ultricies dui et porttitor blandit. Etiam turpis quam, pretium ac convallis a, blandit sit amet ipsum. \
            Sed ut pharetra arcu. Pellentesque id magna sapien. Suspendisse potenti. \
            Donec ut purus venenatis, mollis est ut, sollicitudin egestas.".to_string()
        }
    }
}
