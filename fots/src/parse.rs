//! Grammar Parser
//!
//! Grammar parser parses the input and return the parse tree.

/// Grammar Parser
///
/// Grammar Parser can be used to parse text by single rule.
///```
/// use fots::parse::{GrammarParser, Rule};
/// use pest::Parser;
/// let text = "*[*i8;(0xFF,0xFFFF)]";
/// let re = GrammarParser::parse(Rule::TypeExp, text);
/// assert!(re.is_ok());
/// ```
#[derive(Parser)]
#[grammar = "fots.pest"]
pub struct GrammarParser;

// These test cases need a older version of rustc, ignore for now.
// #[cfg(test)]    // nightly required.
// mod tests {
//     use pest::Parser;

//     use super::*;

//     const I8: &str = "i8";
//     const I16: &str = "i16";
//     const I32: &str = "i32";
//     const STR: &str = "str";
//     const IDENT: &str = "IDENT_DEMO_2020";

//     const PTR_TPL: &str = "*{}";
//     const SLICE_TPL: &str = "[ {};(0x225569fecca,959595959595995) ]";
//     const RES_TPL: &str = "res< {} >";
//     const LEN_TPL: &str = "len<{},top.under.path>";

//     const STRUCT_TPL: &str = " struct  demo_struct {{\n\t arg1:{},\n\t arg2:{},\n\targ3:{} \t}} ";
//     const UNION_TPL: &str = "union demo_union {{\n\t arg1:{}, \n\t arg2:{},\n\targ3:{} \t}}";
//     const FLAG_TPL: &str = "flag demo_union{{ \n\targ1={},\n\t arg2={},\n\targ3={}}}";
//     const ALIAS_TPL: &str = "type {} = {}";
//     const FN_TPL: &str = "fn demo_fn (arg1:{},arg2:{},arg3:{}){}";
//     const RULE_TPL: &str = "rule demo_rule{{  fd=open({},{},{}) }}";

//     #[test]
//     fn type_def() {
//         let ptr = rt_format!(PTR_TPL, IDENT).unwrap(); // *IDENT_DEMO_2020
//         let ptr_1 = rt_format!(PTR_TPL, ptr).unwrap(); // **IDENT_DEMO_2020
//         let slice_1 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
//         let res_1 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
//         let len_1 = rt_format!(LEN_TPL, I32).unwrap();
//         let struct_1 = rt_format!(STRUCT_TPL, res_1, len_1, slice_1).unwrap();
//         let union_1 = rt_format!(UNION_TPL, res_1, len_1, slice_1).unwrap();
//         let flag_1 = rt_format!(FLAG_TPL, 0xFFFFFFF, 0xFFFFFF, 0xFFFFFF).unwrap();
//         let alias_1 = rt_format!(ALIAS_TPL, IDENT, res_1).unwrap();

//         assert!(GrammarParser::parse(Rule::Root, &dbg!(struct_1)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(union_1)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(flag_1)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(alias_1)).is_ok());

//         let ptr_2 = rt_format!(PTR_TPL, ptr_1).unwrap(); // **IDENT_DEMO_2020
//         let slice_2 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
//         let res_2 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
//         let len_2 = rt_format!(LEN_TPL, I32).unwrap();
//         let struct_2 = rt_format!(STRUCT_TPL, res_1, len_1, slice_1).unwrap();
//         let union_2 = rt_format!(UNION_TPL, res_1, len_1, slice_1).unwrap();
//         let flag_2 = rt_format!(FLAG_TPL, 0xFFFFFFF, 0xFFFFFF, 0xFFFFFF).unwrap();
//         let alias_2 = rt_format!(ALIAS_TPL, IDENT, res_1).unwrap();

//         assert!(GrammarParser::parse(Rule::Root, &dbg!(struct_2)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(union_2)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(flag_2)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(alias_2)).is_ok());

//         let ptr_3 = rt_format!(PTR_TPL, ptr_2).unwrap();
//         let slice_3 = rt_format!(SLICE_TPL, ptr_2).unwrap();
//         let res_3 = rt_format!(RES_TPL, slice_2).unwrap();
//         let len_3 = rt_format!(LEN_TPL, I32).unwrap();
//         let struct_3 = rt_format!(STRUCT_TPL, res_2, len_2, slice_2).unwrap();
//         let union_3 = rt_format!(UNION_TPL, res_2, len_2, slice_2).unwrap();
//         let flag_3 = rt_format!(FLAG_TPL, 0xFFFFFFF, 0xFFFFFF, 0xFFFFFF).unwrap();
//         let alias_3 = rt_format!(ALIAS_TPL, IDENT, res_2).unwrap();

//         assert!(GrammarParser::parse(Rule::Root, &dbg!(struct_3)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(union_3)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(flag_3)).is_ok());
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(alias_3)).is_ok());
//     }

//     #[test]
//     fn fn_def() {
//         let ptr = rt_format!(PTR_TPL, IDENT).unwrap(); // *IDENT_DEMO_2020
//         let ptr_1 = rt_format!(PTR_TPL, ptr).unwrap(); // **IDENT_DEMO_2020
//         let slice_1 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
//         let res_1 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
//         let len_1 = rt_format!(LEN_TPL, I32).unwrap();
//         let fn_1 = rt_format!(FN_TPL, ptr_1, slice_1, res_1, len_1).unwrap();
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(fn_1)).is_ok());

//         let ptr_2 = rt_format!(PTR_TPL, ptr_1).unwrap(); // **IDENT_DEMO_2020
//         let slice_2 = rt_format!(SLICE_TPL, ptr_1).unwrap(); // [**IDENT_DEMO_2020, (...)]
//         let res_2 = rt_format!(RES_TPL, slice_1).unwrap(); // Res<[**IDENT_DEMO_2020,(...)]>
//         let len_2 = rt_format!(LEN_TPL, I32).unwrap();
//         let fn_2 = rt_format!(FN_TPL, ptr_2, slice_2, res_2, len_2).unwrap();
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(fn_2)).is_ok());

//         let ptr_3 = rt_format!(PTR_TPL, ptr_2).unwrap();
//         let slice_3 = rt_format!(SLICE_TPL, ptr_2).unwrap();
//         let res_3 = rt_format!(RES_TPL, slice_2).unwrap();
//         let len_3 = rt_format!(LEN_TPL, I32).unwrap();
//         let fn_3 = rt_format!(FN_TPL, ptr_3, res_3, len_3, slice_3).unwrap();
//         assert!(GrammarParser::parse(Rule::Root, &dbg!(fn_3)).is_ok());
//     }
// }
