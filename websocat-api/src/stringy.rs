use anyhow::Context;
use std::collections::HashMap;

use super::Result;

#[derive(Eq, PartialEq, Debug)]
#[cfg_attr(test,derive(Clone))]
pub enum StringOrSubnode {
    Str(String),
    Subnode(StringyNode),
}
#[derive(Eq, PartialEq, Debug)]
#[cfg_attr(test,derive(Clone, proptest_derive::Arbitrary))]
pub struct Ident(
    #[cfg_attr(test, proptest(regex = "[a-z0-9._]+"))]
    pub String
);
/// A part of parsed command line before looking up the SpecifierClasses.
#[derive(Eq, PartialEq, Debug)]
#[cfg_attr(test,derive(Clone, proptest_derive::Arbitrary))]
pub struct StringyNode {
    pub name: Ident,
    pub properties: Vec<(Ident, StringOrSubnode)>,
    pub array: Vec<StringOrSubnode>,
    // pub child_nodes: id_tree::NodeId -- implied,
}


struct ValueForPrinting<'a>(&'a str);

impl<'a> std::fmt::Display for ValueForPrinting<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::with_capacity(self.0.len());
        let mut tainted = false;
        for x in self.0.as_bytes().iter().map(|b|std::ascii::escape_default(*b)) {
            let x : Vec<u8> = x.collect();
            
            if x.len() > 1 { 
                tainted = true;
            } else {
                match x[0] {
                    x if identchar(x) => (),
                    _ => tainted = true,
                }
            }

            s.push_str(&String::from_utf8(x).unwrap());
        }
        if self.0.len() == 0 { tainted = true; }
        if tainted {
            write!(f, "\"{}\"", s)?;
        } else {
            write!(f, "{}", self.0)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for StringyNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}", self.name.0)?;
        for (k, v) in &self.properties {
            match v {
                StringOrSubnode::Str(x) => write!(f, " {}={}", k.0, ValueForPrinting(x))?,
                StringOrSubnode::Subnode(x) => write!(f, " {}={}", k.0, x)?,
            };
        }
        for e in &self.array {
            match e {
                StringOrSubnode::Str(x) => write!(f, " {}", ValueForPrinting(x))?,
                StringOrSubnode::Subnode(x) => write!(f, " {}", x)?,
            };
        }
        write!(f, "]")?;
        Ok(())
    }
}

mod tests;

fn identchar(b: u8) -> bool {
    match b {
        | b'0'..=b'9'
        | b'a'..=b'z'
        | b'A'..=b'Z'
        | b'_' | b':' | b'?' | b'@'
        | b'.' | b'/' | b'#' | b'&'
        | b'\x80' ..= b'\xFF'
        => true,
        _ => false
    }
}

#[rustfmt::skip] // tends to collapse character ranges into one line and to remove trailing `|`s.
impl StringyNode {
    fn read(r: &mut std::iter::Peekable<impl Iterator<Item=u8>>) -> Result<StringyNode> {
        let mut chunk : Vec<u8> = Vec::with_capacity(20);

        if r.next() != Some(b'[') { anyhow::bail!("Tree node must begin with `[` character"); }

        #[derive(Clone,Copy, Eq, PartialEq, Debug)]
        enum S {
            BeforeName,
            Name,
            ForcedSpace,
            Space,
            Chunk,
            ChunkEsc,
            ChunkEscBs,
            ChunkEscHex,
            Finish,
        }

        let mut state = S::BeforeName;

        let mut name : Option<String> = None;
        let mut array: Vec<StringOrSubnode> = vec![];
        let mut properties: Vec<(Ident, StringOrSubnode)> = vec![];
    
        let mut property_name : Option<String> = None;

        let mut hex : tinyvec::ArrayVec<[u8; 2]> = Default::default();

        while let Some(c) = r.peek() {
            //eprintln!("{:?} {}", state, c);
            match state {
                S::Name | S::BeforeName => {
                    match c {
                        x if identchar(*x)
                        => {
                            chunk.push(*c);
                            state = S::Name;
                        }
                        b' ' => {
                            if state == S::Name {
                                name = Some(String::from_utf8(chunk)?);
                                chunk = Vec::with_capacity(20);
                                state = S::Space;
                            } else {
                                // no-op
                            }
                        }
                        b']' => {
                            if state == S::Name {
                                name = Some(String::from_utf8(chunk)?); 
                            } 
                            state = S::Finish;
                            r.next();
                            break;
                        }
                        _ => anyhow::bail!("Invalid character {} while reading tree node name", std::ascii::escape_default(*c)),
                    }
                }
                S::ForcedSpace => {
                    match c {
                        b' ' => {
                            state = S::Space;
                        }
                        b']' => {
                            state = S::Finish;
                            r.next();
                            break;
                        }
                        _ => anyhow::bail!(
                            "Expected a space character or `]` after `\"` or `]`, not {} when parsing node named {}",
                            std::ascii::escape_default(*c),
                            name.as_ref().map(|x|&**x).unwrap_or("???"),
                        ),
                    }
                }
                S::Space => {
                    match c {
                        x if identchar(*x)
                        => {
                            chunk.push(*c);
                            state = S::Chunk;
                        }
                        b'"' => {
                            state = S::ChunkEsc;
                        }
                        b']' => {
                            r.next();
                            state = S::Finish;
                            break;
                        }
                        b'[' => {
                            let subnode = StringyNode::read(r).with_context(||format!(
                                "Failed to read subnode array element {} of node {}",
                                array.len()+1,
                                name.as_ref().map(|x|&**x).unwrap_or("???"),
                            ))?;
                            array.push(StringOrSubnode::Subnode(subnode));
                            state = S::ForcedSpace;
                            continue;
                        }
                        b' ' => {
                            // no-op
                        }
                        _ => anyhow::bail!(
                            "Invalid character {} in tree node named {}",
                            std::ascii::escape_default(*c),
                            name.as_ref().map(|x|&**x).unwrap_or("???")
                        ),
                    }
                }
                S::Chunk => {
                    match c {
                        x if identchar(*x)
                        => {
                            chunk.push(*c);
                        }
                        b' ' | b']' => {
                            if chunk.is_empty() {
                                anyhow::bail!(
                                    "Unescaped empty propery {} value of tree node {}",
                                    property_name.as_ref().map(|x|&**x).unwrap_or("???"),
                                    name.as_ref().map(|x|&**x).unwrap_or("???")
                                );
                            }
                            let ch = String::from_utf8(chunk)?;
                            chunk = Vec::with_capacity(20);
                            if let Some(pn) = property_name {
                                properties.push((Ident(pn), StringOrSubnode::Str(ch)));
                            } else {
                                array.push(StringOrSubnode::Str(ch));
                            }
                            property_name = None;
                            if *c == b']' {
                                state = S::Finish;
                                r.next();
                                break;
                            } else {
                                state = S::Space;
                            }
                        }
                        b'=' => {
                            if property_name.is_some() {
                                anyhow::bail!(
                                    "Duplicate unescaped = character when paring property {} of a tree node {}",
                                    property_name.unwrap(),
                                    name.as_ref().map(|x|&**x).unwrap_or("???")
                                );
                            }
                            let ch = String::from_utf8(chunk)?;
                            property_name = Some(ch);
                            chunk = Vec::with_capacity(20);
                        }
                        b'"' => {
                            if property_name.is_none() || ! chunk.is_empty() {
                                anyhow::bail!(
                                    "Property value `\"` escape character in a tree node named {} must come immediately after `=`",
                                    name.as_ref().map(|x|&**x).unwrap_or("???")
                                );
                            }
                            state = S::ChunkEsc;
                        }
                        b'[' => {
                            if let Some(pn) = property_name {
                                if ! chunk.is_empty() {
                                    anyhow::bail!(
                                        "Wrong `[` character position when parsing a tree node named {}",
                                        name.as_ref().map(|x|&**x).unwrap_or("???")
                                    );
                                }
                                let subnode = StringyNode::read(r).with_context(||format!(
                                    "Failed to read property {} value of node {}",
                                    pn,
                                    name.as_ref().map(|x|&**x).unwrap_or("???"),
                                ))?;
                                properties.push((Ident(pn), StringOrSubnode::Subnode(subnode)));
                                state = S::ForcedSpace;
                                property_name = None;
                                continue;
                            } else {
                                anyhow::bail!(
                                    "Wrong `[` character position when parsing a tree node named {}",
                                    name.as_ref().map(|x|&**x).unwrap_or("???")
                                );
                            }
                        }
                        _ => anyhow::bail!(
                            "Invalid character {} in tree node named {} when a parsing potential property or array element",
                            std::ascii::escape_default(*c),
                            name.as_ref().map(|x|&**x).unwrap_or("???")
                        ),
                    }
                }
                S::ChunkEsc => {
                    match c {
                        b'"' => {
                            let ch = String::from_utf8(chunk)?;
                            chunk = Vec::with_capacity(20);
                            if let Some(pn) = property_name {
                                properties.push((Ident(pn), StringOrSubnode::Str(ch)));
                            } else {
                                array.push(StringOrSubnode::Str(ch));
                            }
                            property_name = None;
                            state = S::ForcedSpace;
                        }
                        b'\\' => {
                            state = S::ChunkEscBs;
                        }
                        _ => {
                            chunk.push(*c);
                        }
                    }
                }
                S::ChunkEscBs => {
                    match c {
                        b't' => chunk.push(b'\t'),
                        b'n' => chunk.push(b'\n'),
                        b'\'' => chunk.push(b'\''),
                        b'"' => chunk.push(b'"'),
                        b'\\' => chunk.push(b'\\'),
                        b'x' => (),
                        _ => anyhow::bail!(
                            "Invalid escape sequence character {} when parsing tree node {}",
                            std::ascii::escape_default(*c),
                            name.as_ref().map(|x|&**x).unwrap_or("???"),
                        ),
                    }
                    state = S::ChunkEsc;
                    if *c == b'x' { state = S::ChunkEscHex; }
                }
                S::ChunkEscHex => {
                    match c {
                        b'0' ..= b'9' => hex.push(*c - b'0'),
                        b'a' ..= b'f' => hex.push(*c + 10 - b'a'),
                        b'A' ..= b'F' => hex.push(*c + 10 - b'A'),
                        _ => anyhow::bail!(
                            "Invalid hex escape sequence character {} when parsing tree node {}",
                            std::ascii::escape_default(*c),
                            name.as_ref().map(|x|&**x).unwrap_or("???"),
                        ),
                    }
                    if hex.len() == 2 {
                        chunk.push(hex[0] * 16 + hex[1]);
                        state = S::ChunkEsc;
                        hex.clear();
                    }
                }
                S::Finish => break,
            }
            r.next();
        }
        if state != S::Finish {
            anyhow::bail!(
                "Trimmed input when parsing the tree node named {}",
                name.as_ref().map(|x|&**x).unwrap_or("???"),
            );
        }
        if name.is_none() {
            anyhow::bail!(
                "Empty tree nodes are not allowed",
            );
        }

        Ok(StringyNode {
            name: Ident(name.unwrap()),
            properties,
            array,
        })
    }
}

impl std::str::FromStr for StringyNode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        StringyNode::read(&mut s.bytes().peekable())
    }
}

impl super::PropertyValueType {
    pub fn interpret(&self, x: &str) -> super::Result<super::PropertyValue> {
        use super::{PropertyValue as PV, PropertyValueType as PVT};
        match self {
            PVT::Stringy => Ok(PV::Stringy(x.to_owned())),
            PVT::Enummy(_) => todo!(),
            PVT::Numbery => Ok(PV::Numbery(x.parse()?)),
            PVT::Floaty => Ok(PV::Floaty(x.parse()?)),
            PVT::Booly => Ok(PV::Booly(x.parse()?)),
            PVT::SockAddr => Ok(PV::SockAddr(x.parse()?)),
            PVT::IpAddr => Ok(PV::IpAddr(x.parse()?)),
            PVT::PortNumber => Ok(PV::PortNumber(x.parse()?)),
            PVT::Path => todo!(),
            PVT::Uri => todo!(),
            PVT::Duration => todo!(),
            PVT::ChildNode => panic!(
                "You can't use PropertyValueType::interpret for obtaining child node pointers"
            ),
        }
    }
}

impl StringyNode {
    fn build_impl(
        &self,
        classes: &super::ClassRegistrar,
        tree: &mut super::Slab<super::NodeId, super::DParsedNode>,
    ) -> Result<super::NodeId> {
        if let Some(cls) = classes.officname_to_classes.get(&self.name.0) {
            let props = cls.properties();
            let mut p: HashMap<String, super::PropertyValueType> =
                HashMap::with_capacity(props.len());
            p.extend(props.into_iter().map(|pi| (pi.name, pi.r#type)));

            let mut b = cls.new_node();

            use super::{PropertyValueType as PVT, PropertyValue as PV};
            use StringOrSubnode::{Str,Subnode};

            for (Ident(k), v) in &self.properties {
                if let Some(typ) = p.get(k) {
                    let vv = match (typ, v) {
                        (PVT::ChildNode, Subnode(x)) => PV::ChildNode(
                            x.build_impl(classes, tree).with_context(||format!(
                                "Building subbnode property {} value of node type {}",
                                k, self.name.0,
                            ))?,
                        ),
                        (_, Subnode(_)) => anyhow::bail!("A subnode is not expected as a property value {} of node {}", k, self.name.0),
                        (PVT::ChildNode, _) => anyhow::bail!("Subnode (`[...]`) expected as a property value {} of node {}", k, self.name.0),
                        (ty, Str(x)) => ty.interpret(x).with_context(|| {
                            format!(
                                "Failed to parse property {} in node {} that has value `{}`",
                                k, self.name.0, x
                            )
                        })?,
                    };
                    b.set_property(k, vv).with_context(|| {
                        format!("Failed to set property {} in node {}", k, self.name.0)
                    })?;
                } else {
                    anyhow::bail!("Property {} of node type {} not found", k, self.name.0);
                }
            }

            let at = cls.array_type();

            for (n, e) in self.array.iter().enumerate() {
                if let Some(at) = &at {
                    let vv = match (at, e) {
                        (PVT::ChildNode, Subnode(x)) => PV::ChildNode(
                            x.build_impl(classes, tree).with_context(||format!(
                                "Building subnode array element number {} in node {}",
                                n, self.name.0,
                            ))?,
                        ),
                        (PVT::ChildNode, _) => anyhow::bail!("Subnode (`[...]`) expected as an array element number {} of node {}", n, self.name.0),
                        (ty, Str(x)) => ty.interpret(x).with_context(|| {
                            format!(
                                "Failed to array element number {} in node {} that has value `{}`",
                                n, self.name.0, x
                            )
                        })?,
                        (_, Subnode(_)) => anyhow::bail!("A subnode is not expected as an array element number {} of node {}", n, self.name.0),
                    };

                    b.push_array_element(vv)
                    .with_context(|| {
                        format!("Failed to push array element number `{}` to node {}", n, self.name.0)
                    })?;
                } else {
                    anyhow::bail!("Node type {} does not support array elements", self.name.0);
                }
            }

            Ok(tree.insert(b.finish()?))
        } else {
            anyhow::bail!("Node type {} not found", self.name.0)
        }
    }

    pub fn build(
        &self,
        classes: &super::ClassRegistrar,
        tree: &mut super::Tree,
    ) -> Result<super::NodeId> {
        self.build_impl(classes, tree)
    }

    /// Turn parsed node back into it's stringy representation
    pub fn reverse(root: super::NodeId, tree:&super::Tree) -> Result<Self> {
        let n = tree.get(root).with_context(||format!("Node not found"))?;
        let c = n.class();

        let name = Ident(c.official_name());
        let mut properties : Vec<(Ident, StringOrSubnode)> = Vec::new();
        let mut array : Vec<StringOrSubnode> = Vec::new();

        for super::PropertyInfo { name: pn, help: _, r#type } in c.properties() {
            if let Some(v) = n.get_property(&pn) {
                let sn = match (v, r#type) {
                    (super::PropertyValue::ChildNode(q), super::PropertyValueType::ChildNode) => {
                        StringOrSubnode::Subnode(StringyNode::reverse(q, tree)?)
                    },
                    (_, super::PropertyValueType::ChildNode) => {
                        anyhow::bail!("Inconsistent property value for {} in node type {}", pn, name.0)
                    }
                    (super::PropertyValue::ChildNode(_), _) => {
                        anyhow::bail!("Inconsistent property value for {} in node type {}", pn, name.0)
                    }
                    (opv, _) => {
                        StringOrSubnode::Str(format!("{}", opv))
                    }
                };
                properties.push((Ident(pn), sn));
            }
        }

        for el in n.get_array() {
            let sn = match (el, c.array_type()) {
                (_, None) => {
                    anyhow::bail!("No array elements expected in node type {}", name.0)
                }
                (super::PropertyValue::ChildNode(q), Some(super::PropertyValueType::ChildNode)) => {
                    StringOrSubnode::Subnode(StringyNode::reverse(q, tree)?)
                },
                (_, Some(super::PropertyValueType::ChildNode)) => {
                    anyhow::bail!("Inconsistent array elment value in node type {}", name.0)
                }
                (super::PropertyValue::ChildNode(_), _) => {
                    anyhow::bail!("Inconsistent array element value in node type {}", name.0)
                }
                (opv, Some(_)) => {
                    StringOrSubnode::Str(format!("{}", opv))
                }
            };
            array.push(sn);
        }

        Ok(StringyNode {
            name,
            properties,
            array,
        })
    }
}

impl std::fmt::Display for super::PropertyValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            crate::PropertyValue::Stringy(x) => x.fmt(f),
            crate::PropertyValue::Enummy(_) => todo!(),
            crate::PropertyValue::Numbery(x) => x.fmt(f),
            crate::PropertyValue::Floaty(x) => x.fmt(f),
            crate::PropertyValue::Booly(x) => x.fmt(f),
            crate::PropertyValue::SockAddr(x) => x.fmt(f),
            crate::PropertyValue::IpAddr(x) => x.fmt(f),
            crate::PropertyValue::PortNumber(x) => x.fmt(f),
            crate::PropertyValue::Path(x) => match x.to_str() {
                Some(y) => y.fmt(f),
                None => "(?:/??)".fmt(f),
            },
            crate::PropertyValue::Uri(x) => x.fmt(f),
            crate::PropertyValue::Duration(_) => todo!(),
            crate::PropertyValue::ChildNode(_) => write!(f, "[???]"),
        }
    }
}