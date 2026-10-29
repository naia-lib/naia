// use std::fmt;
//
// // SCHEME definitions from RFC 7064 Section 3.2.
//
// // URI as defined in RFC 7064.
// #[derive(PartialEq, Debug)]
// pub(crate) struct Uri {
//     pub(crate) scheme: String,
//     pub(crate) host: String,
//     pub(crate) port: Option<u16>,
// }
//
// impl fmt::Display for Uri {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         let host = if self.host.contains("::") {
//             "[".to_owned() + self.host.as_str() + "]"
//         } else {
//             self.host.clone()
//         };
//
//         if let Some(port) = self.port {
//             write!(f, "{}:{}:{}", self.scheme, host, port)
//         } else {
//             write!(f, "{}:{}", self.scheme, host)
//         }
//     }
// }
