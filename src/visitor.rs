use std::slice;

use crate::common::Text;
use crate::query::{SelectionSet, Directive, Selection, Field};
use crate::query::{Document, Definition};


pub trait Visit {
    fn visit<'x, D: 'x>(&'x self) -> <(&'x Self, &'x D) as VisitorData>::Data
        where (&'x Self, &'x D): VisitorData,
            <(&'x Self, &'x D) as VisitorData>::Data: CreateData<'x, &'x Self, &'x D>,
    {
        CreateData::new(self)
    }
}

impl<S> Visit for S { }

pub trait VisitorData {
    type Data;
}

#[derive(Debug)]
pub struct FieldIter<'a, T>
    where T: Text<'a>
{
    stack: Vec<slice::Iter<'a, Selection<'a, T>>>,
}

pub trait CreateData<'a, S: ?Sized, D: ?Sized> {
    fn new(v: S) -> Self;
}

impl<'a, T> CreateData<'a, &'a SelectionSet<'a, T>, &'a Field<'a, T>>
    for FieldIter<'a, T>
    where T: Text<'a>,
{
    fn new(v: &'a SelectionSet<'a, T>) -> Self {
        FieldIter {
            stack: vec![v.items.iter()],
        }
    }
}

impl<'a, T> VisitorData for (&'a SelectionSet<'a, T>, &'a Field<'a, T>)
    where T: Text<'a>,
{
    type Data = FieldIter<'a, T>;
}

impl<'a, T: 'a> Iterator for FieldIter<'a, T>
    where T: Text<'a>,
{
    type Item = &'a Field<'a, T>;
    fn next(&mut self) -> Option<&'a Field<'a, T>> {
        let ref mut stack = self.stack;
        while !stack.is_empty() {
            match stack.last_mut().and_then(|iter| iter.next()) {
                Some(Selection::Field(f)) => {
                    stack.push(f.selection_set.items.iter());
                    return Some(f);
                }
                Some(Selection::InlineFragment(f)) => {
                    stack.push(f.selection_set.items.iter());
                    continue;
                }
                Some(Selection::FragmentSpread(..)) => {}
                None => {
                    stack.pop();
                }
            }
        }
        return None;
    }
}

#[derive(Debug)]
pub struct DocumentFieldIter<'a, T>
    where T: Text<'a>
{
    doc_iter: slice::Iter<'a, Definition<'a, T>>,
    field_iter: Option<FieldIter<'a, T>>,
}

impl<'a, T> VisitorData for (&'a Document<'a, T>, &'a Field<'a, T>)
    where T: Text<'a>,
{
    type Data = DocumentFieldIter<'a, T>;
}

impl<'a, T> CreateData<'a, &'a Document<'a, T>, &'a Field<'a, T>>
    for DocumentFieldIter<'a, T>
    where T: Text<'a>,
{
    fn new(v: &'a Document<'a, T>) -> Self {
        Self {
            doc_iter: v.definitions.iter(),
            field_iter: None,
        }
    }
}

impl<'a, T: 'a> Iterator for DocumentFieldIter<'a, T>
    where T: Text<'a>,
{
    type Item = &'a Field<'a, T>;
    fn next(&mut self) -> Option<&'a Field<'a, T>> {
        use crate::query::Definition::*;
        loop {
            if let Some(field_iter) = &mut self.field_iter {
                if let Some(result) = field_iter.next() {
                    return Some(result);
                }
            }
            self.field_iter.take();
            let ss = match self.doc_iter.next() {
                Some(Operation(def)) => &def.selection_set,
                Some(Fragment(def)) => &def.selection_set,
                None => return None,
            };
            self.field_iter = Some(ss.visit::<Field<'a, T>>());
        }
    }
}


#[derive(Debug)]
pub struct SetDirectiveIter<'a, T>
    where T: Text<'a>
{
    stack: Vec<slice::Iter<'a, Selection<'a, T>>>,
    directive_iter: Option<slice::Iter<'a, Directive<'a, T>>>,
}

impl<'a, T> VisitorData for (&'a SelectionSet<'a, T>, &'a Directive<'a, T>)
    where T: Text<'a>,
{
    type Data = SetDirectiveIter<'a, T>;
}

impl<'a, T> CreateData<'a, &'a SelectionSet<'a, T>, &'a Directive<'a, T>>
    for SetDirectiveIter<'a, T>
    where T: Text<'a>,
{
    fn new(v: &'a SelectionSet<'a, T>) -> Self {
        Self {
            stack: vec![v.items.iter()],
            directive_iter: None,
        }
    }
}

impl<'a, T: 'a> Iterator for SetDirectiveIter<'a, T>
    where T: Text<'a>,
{
    type Item = &'a Directive<'a, T>;
    fn next(&mut self) -> Option<&'a Directive<'a, T>> {
        'outer: loop {
            if let Some(directive_iter) = &mut self.directive_iter {
                if let Some(result) = directive_iter.next() {
                    return Some(result);
                }
            }
            self.directive_iter.take();
            let ref mut stack = self.stack;
            while !stack.is_empty() {
                match stack.last_mut().and_then(|iter| iter.next()) {
                    Some(Selection::Field(f)) => {
                        stack.push(f.selection_set.items.iter());
                        self.directive_iter = Some(f.directives.iter());
                        continue 'outer;
                    }
                    Some(Selection::InlineFragment(f)) => {
                        stack.push(f.selection_set.items.iter());
                        self.directive_iter = Some(f.directives.iter());
                        continue 'outer;
                    }
                    Some(Selection::FragmentSpread(f)) => {
                        self.directive_iter = Some(f.directives.iter());
                        continue 'outer;
                    }
                    None => {
                        stack.pop();
                    }
                }
            }
            return None;
        }
    }
}

#[test]
fn test_field_iter() {
    use crate::parse_query;

    let doc = parse_query::<&str>(r#"
        query TestQuery {
            users {
                id
                country {
                    id
                }
            }
        }
    "#).expect("Failed to parse query");
    let mut fields = 0;
    let mut field_names = Vec::new();
    for f in doc.visit::<Field<_>>() {
        fields += 1;
        field_names.push(f.name);
    }
    assert_eq!(fields, 4);
    assert_eq!(field_names, vec!["users", "id", "country", "id"]);
}


#[test]
fn test_dir_iter() {
    use crate::parse_query;
    use crate::query::Definition::Operation;

    let doc = parse_query::<&str>(r#"
        query TestQuery {
            users {
                id @skip(if: false)
                country @include(if: true) {
                    id
                }
            }
        }
    "#).expect("Failed to parse query");
    let def = match doc.definitions.iter().next().unwrap() {
        Operation(op) => &op.selection_set,
        _ => unreachable!(),
    };
    let mut directives = 0;
    for _ in def.visit::<Directive<_>>() {
        directives += 1;
    }
    assert_eq!(directives, 2);
}


