use crate::types;
use std::error::Error;
use pest::Parser;
use pest::iterators::{Pair, Pairs};
use crate::types::{Fots, TypeInfo, TypeId, Field};

/// Parse grammar
#[derive(Parser)]
#[grammar = "fots.pest"]
pub struct Grammar;

/// Parse declaration
/// ```
/// use fots::parse::Content;
/// let text = "struct foo { arg1:i8, arg2:*[i8] }";
/// let re = Content::parse(text);
/// assert!(re.is_ok());
/// ```
pub struct Content;

impl Content {
    fn new() -> Self {
        Content
    }

    /// Parse plain text based on grammar, return all declarations in text
    pub fn parse<E: Error>(text: &str) -> Result<types::Fots, E> {
        let mut parse_tree = match Grammar::parse(Rule::Root, text) {
            Ok(tree) => tree,
            Err(e) => { return Err(Content::report(e)); }
        };
        let mut parser = Content::new();

        for p in parse_tree {
            match p.as_rule() {
                Rule::TypeDef => parser.parse_type(p),
                Rule::FuncDef => parser.parse_func(p),
                Rule::GroupDef => parser.parse_group(p),
                Rule::RuleDef => parser.parse_rule(p),
                _ => unreachable!()
            }
        }
        parser.finish()
    }

    fn finish<E: Error>(&self) -> Result<types::Fots, E> {
        todo!()
    }

    fn parse_func(&mut self, mut p: Pair<Rule>) {
        todo!()
    }

    fn parse_group(&mut self, mut p: Pair<Rule>) {
        todo!()
    }

    fn parse_rule(&mut self, mut p: Pair<Rule>) {
        todo!()
    }

    fn parse_type(&mut self, mut p: Pair<Rule>) {
        let mut p: Pair<Rule> = p.into_inner().next().unwrap();

        match p.as_rule() {
            Rule::StructDef => self.parse_struct(p),
            Rule::UnionDef => self.parse_union(p),
            Rule::FlagDef => self.parse_flag(p),
            Rule::AliasDef => self.parse_alias(p),
            _ => unreachable!()
        }
    }

    fn parse_struct(&mut self, p: Pair<Rule>) {
        let mut p: Pairs<Rule> = p.into_inner();
        let ident_p: Pair<Rule> = p.next().unwrap();
        let mut info = TypeInfo::struct_info(ident_p.as_str());
        let fields_p: Pair<Rule> = p.next().unwrap();
        assert_eq!(fields_p.as_rule(), Rule::Fields);

        // parse each field
        for field_p in fields_p.into_inner() {
            let mut ps = field_p.into_inner();
            let mut ident_p: Pair<Rule> = ps.next().unwrap();
            let mut type_exp_p = ps.next().unwrap();
            info.add_field(Field::new(ident_p.as_str(), self.parse_type_exp(type_exp_p)));
        }
        todo!()
    }

    fn parse_type_exp(&mut self, p: Pair<Rule>) -> TypeId {
        todo!()
    }

    fn parse_union(&mut self, p: Pair<Rule>) {
        todo!()
    }

    fn parse_flag(&mut self, p: Pair<Rule>) {
        todo!()
    }

    fn parse_alias(&mut self, p: Pair<Rule>) {
        todo!()
    }

    fn report<E: Error>(e: pest::error::Error<Rule>) -> E {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const I8: &str = "i8";
    const I16: &str = "i16";
    const I32: &str = "i32";
    const STR: &str = "str";
    const IDENT: &str = "IDENT_DEMO_2020";

    const PTR_TPL: &str = "*{}";
    const SLICE_TPL: &str = "[ {};(0x225569fecca,959595959595995) ]";
    const RES_TPL: &str = "res< {} >";
    const LEN_TPL: &str = "len<{},top.under.path>";

    const STRUCT_TPL: &str = " struct  demo_struct {{\n\t arg1:{},\n\t arg2:{},\n\targ3:{} \t}} ";
    const UNION_TPL: &str = "union demo_union {{\n\t arg1:{}, \n\t arg2:{},\n\targ3:{} \t}}";
    const FLAG_TPL: &str = "flag demo_union{{ \n\targ1={},\n\t arg2={},\n\targ3={}}}";
    const ALIAS_TPL: &str = "type {} = {}";
    const FN_TPL: &str = "fn demo_fn (arg1:{},arg2:{},arg3:{}){}";
    const RULE_TPL: &str = "rule demo_rule{{  fd=open({},{},{}) }}";

    #[test]
    fn type_def() {
        let ptr = rt_format!(PTR_TPL, IDENT).unwrap(); // *IDENT_DEMO_2020
        let ptr_1 = rt_format!(PTR_TPL, ptr).unwrap(); // **IDENT_DEMO_2020
        let slice_1 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
        let res_1 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
        let len_1 = rt_format!(LEN_TPL, I32).unwrap();
        let struct_1 = rt_format!(STRUCT_TPL, res_1, len_1, slice_1).unwrap();
        let union_1 = rt_format!(UNION_TPL, res_1, len_1, slice_1).unwrap();
        let flag_1 = rt_format!(FLAG_TPL, 0xFFFFFFF, 0xFFFFFF, 0xFFFFFF).unwrap();
        let alias_1 = rt_format!(ALIAS_TPL, IDENT, res_1).unwrap();

        assert!(GrammarParser::parse(Rule::Fots, &dbg!(struct_1)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(union_1)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(flag_1)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(alias_1)).is_ok());

        let ptr_2 = rt_format!(PTR_TPL, ptr_1).unwrap(); // **IDENT_DEMO_2020
        let slice_2 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
        let res_2 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
        let len_2 = rt_format!(LEN_TPL, I32).unwrap();
        let struct_2 = rt_format!(STRUCT_TPL, res_1, len_1, slice_1).unwrap();
        let union_2 = rt_format!(UNION_TPL, res_1, len_1, slice_1).unwrap();
        let flag_2 = rt_format!(FLAG_TPL, 0xFFFFFFF, 0xFFFFFF, 0xFFFFFF).unwrap();
        let alias_2 = rt_format!(ALIAS_TPL, IDENT, res_1).unwrap();

        assert!(GrammarParser::parse(Rule::Fots, &dbg!(struct_2)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(union_2)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(flag_2)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(alias_2)).is_ok());

        let ptr_3 = rt_format!(PTR_TPL, ptr_2).unwrap();
        let slice_3 = rt_format!(SLICE_TPL, ptr_2).unwrap();
        let res_3 = rt_format!(RES_TPL, slice_2).unwrap();
        let len_3 = rt_format!(LEN_TPL, I32).unwrap();
        let struct_3 = rt_format!(STRUCT_TPL, res_2, len_2, slice_2).unwrap();
        let union_3 = rt_format!(UNION_TPL, res_2, len_2, slice_2).unwrap();
        let flag_3 = rt_format!(FLAG_TPL, 0xFFFFFFF, 0xFFFFFF, 0xFFFFFF).unwrap();
        let alias_3 = rt_format!(ALIAS_TPL, IDENT, res_2).unwrap();

        assert!(GrammarParser::parse(Rule::Fots, &dbg!(struct_3)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(union_3)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(flag_3)).is_ok());
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(alias_3)).is_ok());
    }

    #[test]
    fn fn_def() {
        let ptr = rt_format!(PTR_TPL, IDENT).unwrap(); // *IDENT_DEMO_2020
        let ptr_1 = rt_format!(PTR_TPL, ptr).unwrap(); // **IDENT_DEMO_2020
        let slice_1 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
        let res_1 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
        let len_1 = rt_format!(LEN_TPL, I32).unwrap();
        let fn_1 = rt_format!(FN_TPL, ptr_1, slice_1, res_1, len_1).unwrap();
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(fn_1)).is_ok());

        let ptr_2 = rt_format!(PTR_TPL, ptr_1).unwrap(); // **IDENT_DEMO_2020
        let slice_2 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
        let res_2 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
        let len_2 = rt_format!(LEN_TPL, I32).unwrap();
        let fn_2 = rt_format!(FN_TPL, ptr_2, slice_2, res_2, len_2).unwrap();
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(fn_2)).is_ok());

        let ptr_3 = rt_format!(PTR_TPL, ptr_2).unwrap();
        let slice_3 = rt_format!(SLICE_TPL, ptr_2).unwrap();
        let res_3 = rt_format!(RES_TPL, slice_2).unwrap();
        let len_3 = rt_format!(LEN_TPL, I32).unwrap();
        let fn_3 = rt_format!(FN_TPL, ptr_3, res_3, len_3, slice_3).unwrap();
        assert!(GrammarParser::parse(Rule::Fots, &dbg!(fn_3)).is_ok());
    }
}
