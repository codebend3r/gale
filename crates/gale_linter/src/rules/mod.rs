//! Every built-in rule.
//!
//! Each rule lives in its own module, declared below, and is listed once in
//! the `rules!` table at the end of this file.  The table generates
//! [`register_all`] and [`RULES`], which `gale_config` builds `gale:all` from,
//! so adding a rule means adding its module and its line in the table.  A test
//! fails if a rule file is missing from the table.

pub mod alpha_value_notation;
pub mod annotation_no_unknown;
pub mod at_rule_allowed_list;
pub mod at_rule_descriptor_no_unknown;
pub mod at_rule_descriptor_value_no_unknown;
pub mod at_rule_disallowed_list;
pub mod at_rule_empty_line_before;
pub mod at_rule_no_deprecated;
pub mod at_rule_no_unknown;
pub mod at_rule_no_vendor_prefix;
pub mod at_rule_prelude_no_invalid;
pub mod at_rule_property_required_list;
pub mod block_no_empty;
pub mod block_no_redundant_nested_style_rules;
pub mod color_function_alias_notation;
pub mod color_function_notation;
pub mod color_hex_alpha;
pub mod color_hex_case;
pub mod color_hex_length;
pub mod color_named;
pub mod color_no_hex;
pub mod color_no_invalid_hex;
pub mod comment_empty_line_before;
pub mod comment_no_empty;
pub mod comment_pattern;
pub mod comment_whitespace_inside;
pub mod comment_word_disallowed_list;
pub mod container_name_pattern;
pub mod csstools_value_no_unknown_custom_properties;
pub mod custom_media_pattern;
pub mod custom_property_empty_line_before;
pub mod custom_property_no_missing_var_function;
pub mod custom_property_pattern;
pub mod declaration_block_no_duplicate_custom_properties;
pub mod declaration_block_no_duplicate_properties;
pub mod declaration_block_no_redundant_longhand_properties;
pub mod declaration_block_no_shorthand_property_overrides;
pub mod declaration_block_single_line_max_declarations;
pub mod declaration_empty_line_before;
pub mod declaration_no_important;
pub mod declaration_property_max_values;
pub mod declaration_property_unit_allowed_list;
pub mod declaration_property_unit_disallowed_list;
pub mod declaration_property_value_allowed_list;
pub mod declaration_property_value_disallowed_list;
pub mod declaration_property_value_keyword_no_deprecated;
pub mod declaration_property_value_no_unknown;
pub mod display_notation;
pub mod font_family_name_quotes;
pub mod font_family_no_duplicate_names;
pub mod font_family_no_missing_generic_family_keyword;
pub mod font_weight_notation;
pub mod function_allowed_list;
pub mod function_calc_no_unspaced_operator;
pub mod function_disallowed_list;
pub mod function_linear_gradient_no_nonstandard_direction;
pub mod function_name_case;
pub mod function_no_unknown;
pub mod function_url_no_scheme_relative;
pub mod function_url_quotes;
pub mod function_url_scheme_allowed_list;
pub mod function_url_scheme_disallowed_list;
pub mod hue_degree_notation;
pub mod import_notation;
pub mod keyframe_block_no_duplicate_selectors;
pub mod keyframe_declaration_no_important;
pub mod keyframe_selector_notation;
pub mod keyframes_name_pattern;
pub mod layer_name_pattern;
pub mod length_zero_no_unit;
pub mod lightness_notation;
pub mod material_no_prefixes;
pub mod max_line_length;
pub mod max_nesting_depth;
pub mod media_feature_name_allowed_list;
pub mod media_feature_name_disallowed_list;
pub mod media_feature_name_no_unknown;
pub mod media_feature_name_no_vendor_prefix;
pub mod media_feature_name_unit_allowed_list;
pub mod media_feature_name_value_allowed_list;
pub mod media_feature_name_value_no_unknown;
pub mod media_feature_range_notation;
pub mod media_query_no_invalid;
pub mod media_type_no_deprecated;
pub mod named_grid_areas_no_invalid;
pub mod nesting_selector_no_missing_scoping_root;
pub mod no_descending_specificity;
pub mod no_duplicate_at_import_rules;
pub mod no_duplicate_selectors;
pub mod no_empty_source;
pub mod no_invalid_double_slash_comments;
pub mod no_invalid_position_at_import_rule;
pub mod no_invalid_position_declaration;
pub mod no_irregular_whitespace;
pub mod no_unknown_animations;
pub mod number_max_precision;
pub mod order_order;
pub mod order_properties_alphabetical_order;
pub mod order_properties_order;
pub mod plugin_browser_compat;
pub mod plugin_enforce_variable_for_property;
pub mod plugin_no_unknown_custom_properties;
pub mod plugin_no_unused_custom_properties;
pub mod plugin_require_file_header_comment;
pub mod property_allowed_list;
pub mod property_disallowed_list;
pub mod property_no_deprecated;
pub mod property_no_unknown;
pub mod property_no_vendor_prefix;
pub mod rule_empty_line_before;
pub mod rule_nesting_at_rule_required_list;
pub mod rule_selector_property_disallowed_list;
pub mod selector_anb_no_unmatchable;
pub mod selector_attribute_name_disallowed_list;
pub mod selector_attribute_operator_allowed_list;
pub mod selector_attribute_operator_disallowed_list;
pub mod selector_attribute_quotes;
pub mod selector_class_pattern;
pub mod selector_combinator_allowed_list;
pub mod selector_combinator_disallowed_list;
pub mod selector_disallowed_list;
pub mod selector_id_pattern;
pub mod selector_max_attribute;
pub mod selector_max_class;
pub mod selector_max_combinators;
pub mod selector_max_compound_selectors;
pub mod selector_max_id;
pub mod selector_max_pseudo_class;
pub mod selector_max_specificity;
pub mod selector_max_type;
pub mod selector_max_universal;
pub mod selector_nested_pattern;
pub mod selector_no_invalid;
pub mod selector_no_qualifying_type;
pub mod selector_no_unmatchable;
pub mod selector_no_vendor_prefix;
pub mod selector_not_notation;
pub mod selector_pseudo_class_allowed_list;
pub mod selector_pseudo_class_disallowed_list;
pub mod selector_pseudo_class_no_unknown;
pub mod selector_pseudo_element_allowed_list;
pub mod selector_pseudo_element_colon_notation;
pub mod selector_pseudo_element_disallowed_list;
pub mod selector_pseudo_element_no_unknown;
pub mod selector_type_case;
pub mod selector_type_no_unknown;
pub mod shorthand_property_no_redundant_values;
pub mod string_no_newline;
pub mod string_quotes;
pub mod syntax_string_no_invalid;
pub mod time_min_milliseconds;
pub mod unit_allowed_list;
pub mod unit_disallowed_list;
pub mod unit_no_unknown;
pub mod value_keyword_case;
pub mod value_no_vendor_prefix;

// Spectrum tools custom plugin rules
pub mod spectrum_tools_no_unknown_custom_properties;

// @stylistic rules
pub mod stylistic_at_rule_name_case;
pub mod stylistic_at_rule_name_space_after;
pub mod stylistic_at_rule_semicolon_newline_after;
pub mod stylistic_at_rule_semicolon_space_before;
pub mod stylistic_block_closing_brace_empty_line_before;
pub mod stylistic_block_closing_brace_newline_after;
pub mod stylistic_block_closing_brace_newline_before;
pub mod stylistic_block_closing_brace_space_before;
pub mod stylistic_block_opening_brace_newline_after;
pub mod stylistic_block_opening_brace_space_after;
pub mod stylistic_block_opening_brace_space_before;
pub mod stylistic_color_hex_case;
pub mod stylistic_declaration_bang_space_after;
pub mod stylistic_declaration_bang_space_before;
pub mod stylistic_declaration_block_semicolon_newline_after;
pub mod stylistic_declaration_block_semicolon_newline_before;
pub mod stylistic_declaration_block_semicolon_space_after;
pub mod stylistic_declaration_block_semicolon_space_before;
pub mod stylistic_declaration_block_trailing_semicolon;
pub mod stylistic_declaration_colon_newline_after;
pub mod stylistic_declaration_colon_space_after;
pub mod stylistic_declaration_colon_space_before;
pub mod stylistic_function_comma_newline_after;
pub mod stylistic_function_comma_space_after;
pub mod stylistic_function_comma_space_before;
pub mod stylistic_function_max_empty_lines;
pub mod stylistic_function_parentheses_newline_inside;
pub mod stylistic_function_parentheses_space_inside;
pub mod stylistic_function_whitespace_after;
pub mod stylistic_indentation;
pub mod stylistic_max_empty_lines;
pub mod stylistic_media_feature_colon_space_after;
pub mod stylistic_media_feature_colon_space_before;
pub mod stylistic_media_feature_name_case;
pub mod stylistic_media_feature_parentheses_space_inside;
pub mod stylistic_media_feature_range_operator_space_after;
pub mod stylistic_media_feature_range_operator_space_before;
pub mod stylistic_media_query_list_comma_newline_after;
pub mod stylistic_media_query_list_comma_space_after;
pub mod stylistic_media_query_list_comma_space_before;
pub mod stylistic_no_empty_first_line;
pub mod stylistic_no_eol_whitespace;
pub mod stylistic_no_extra_semicolons;
pub mod stylistic_no_missing_end_of_source_newline;
pub mod stylistic_number_leading_zero;
pub mod stylistic_number_no_trailing_zeros;
pub mod stylistic_property_case;
pub mod stylistic_selector_attribute_brackets_space_inside;
pub mod stylistic_selector_attribute_operator_space_after;
pub mod stylistic_selector_attribute_operator_space_before;
pub mod stylistic_selector_combinator_space_after;
pub mod stylistic_selector_combinator_space_before;
pub mod stylistic_selector_descendant_combinator_no_non_space;
pub mod stylistic_selector_list_comma_newline_after;
pub mod stylistic_selector_list_comma_newline_before;
pub mod stylistic_selector_list_comma_space_after;
pub mod stylistic_selector_list_comma_space_before;
pub mod stylistic_selector_max_empty_lines;
pub mod stylistic_selector_pseudo_class_case;
pub mod stylistic_selector_pseudo_class_parentheses_space_inside;
pub mod stylistic_selector_pseudo_element_case;
pub mod stylistic_string_quotes;
pub mod stylistic_unicode_bom;
pub mod stylistic_unit_case;
pub mod stylistic_value_list_comma_newline_after;
pub mod stylistic_value_list_comma_newline_before;
pub mod stylistic_value_list_comma_space_after;
pub mod stylistic_value_list_comma_space_before;
pub mod stylistic_value_list_max_empty_lines;

// SCSS-specific rules (scss/ prefix)
pub mod scss_at_else_closing_brace_newline_after;
pub mod scss_at_else_closing_brace_space_after;
pub mod scss_at_else_empty_line_before;
pub mod scss_at_else_if_parentheses_space_before;
pub mod scss_at_extend_no_missing_placeholder;
pub mod scss_at_function_parentheses_space_before;
pub mod scss_at_function_pattern;
pub mod scss_at_if_closing_brace_newline_after;
pub mod scss_at_if_closing_brace_space_after;
pub mod scss_at_if_no_null;
pub mod scss_at_import_partial_extension;
pub mod scss_at_import_partial_extension_disallowed_list;
pub mod scss_at_mixin_argumentless_call_parentheses;
pub mod scss_at_mixin_disallowed_list;
pub mod scss_at_mixin_parentheses_space_before;
pub mod scss_at_mixin_pattern;
pub mod scss_at_rule_conditional_no_parentheses;
pub mod scss_at_rule_no_unknown;
pub mod scss_comment_no_empty;
pub mod scss_comment_no_loud;
pub mod scss_declaration_nested_properties;
pub mod scss_declaration_nested_properties_no_divided_groups;
pub mod scss_dollar_variable_colon_space_after;
pub mod scss_dollar_variable_colon_space_before;
pub mod scss_dollar_variable_empty_line_before;
pub mod scss_dollar_variable_no_missing_interpolation;
pub mod scss_dollar_variable_pattern;
pub mod scss_double_slash_comment_empty_line_before;
pub mod scss_double_slash_comment_inline;
pub mod scss_double_slash_comment_whitespace_inside;
pub mod scss_function_disallowed_list;
pub mod scss_function_no_unknown;
pub mod scss_function_quote_no_quoted_strings_inside;
pub mod scss_function_unquote_no_unquoted_strings_inside;
pub mod scss_load_no_partial_leading_underscore;
pub mod scss_load_partial_extension;
pub mod scss_no_duplicate_dollar_variables;
pub mod scss_no_duplicate_mixins;
pub mod scss_no_global_function_names;
pub mod scss_operator_no_newline_after;
pub mod scss_operator_no_newline_before;
pub mod scss_operator_no_unspaced;
pub mod scss_partial_no_import;
pub mod scss_percent_placeholder_pattern;
pub mod scss_selector_no_redundant_nesting_selector;

use crate::registry::RuleRegistry;
use crate::rule::Rule;

/// A built-in rule, as the `rules!` table lists it.
#[derive(Clone, Copy)]
pub struct BuiltinRule {
  /// The rule itself, for its name and metadata.
  pub rule: &'static dyn Rule,
}

/// Generate [`RULES`] and [`register_all`] from one list of rule types.
///
/// Each entry is `module::Type`, in registration order.
macro_rules! rules {
  ($($module:ident :: $rule:ident),+ $(,)?) => {
    /// Every built-in rule, in registration order.
    pub static RULES: &[BuiltinRule] = &[$(BuiltinRule { rule: &$module::$rule }),+];

    /// Register all built-in rules in the given registry, in the order of
    /// [`RULES`].
    pub fn register_all(registry: &mut RuleRegistry) {
      $(registry.register(Box::new($module::$rule));)+
    }

    /// The module of every rule in [`RULES`], to check that no rule file is
    /// left out of the table.
    #[cfg(test)]
    const MODULES: &[&str] = &[$(stringify!($module)),+];
  };
}

rules! {
  alpha_value_notation::AlphaValueNotation,
  annotation_no_unknown::AnnotationNoUnknown,
  at_rule_allowed_list::AtRuleAllowedList,
  at_rule_descriptor_no_unknown::AtRuleDescriptorNoUnknown,
  at_rule_descriptor_value_no_unknown::AtRuleDescriptorValueNoUnknown,
  at_rule_disallowed_list::AtRuleDisallowedList,
  at_rule_empty_line_before::AtRuleEmptyLineBefore,
  at_rule_no_deprecated::AtRuleNoDeprecated,
  at_rule_no_unknown::AtRuleNoUnknown,
  at_rule_no_vendor_prefix::AtRuleNoVendorPrefix,
  at_rule_prelude_no_invalid::AtRulePreludeNoInvalid,
  at_rule_property_required_list::AtRulePropertyRequiredList,
  block_no_empty::BlockNoEmpty,
  block_no_redundant_nested_style_rules::BlockNoRedundantNestedStyleRules,
  color_function_alias_notation::ColorFunctionAliasNotation,
  color_function_notation::ColorFunctionNotation,
  color_hex_alpha::ColorHexAlpha,
  color_hex_case::ColorHexCase,
  color_hex_length::ColorHexLength,
  color_named::ColorNamed,
  color_no_hex::ColorNoHex,
  color_no_invalid_hex::ColorNoInvalidHex,
  comment_empty_line_before::CommentEmptyLineBefore,
  csstools_value_no_unknown_custom_properties::CsstoolsValueNoUnknownCustomProperties,
  comment_no_empty::CommentNoEmpty,
  comment_pattern::CommentPattern,
  comment_whitespace_inside::CommentWhitespaceInside,
  comment_word_disallowed_list::CommentWordDisallowedList,
  container_name_pattern::ContainerNamePattern,
  custom_media_pattern::CustomMediaPattern,
  custom_property_empty_line_before::CustomPropertyEmptyLineBefore,
  custom_property_no_missing_var_function::CustomPropertyNoMissingVarFunction,
  custom_property_pattern::CustomPropertyPattern,
  declaration_block_no_duplicate_custom_properties::DeclarationBlockNoDuplicateCustomProperties,
  declaration_block_no_duplicate_properties::DeclarationBlockNoDuplicateProperties,
  declaration_block_no_redundant_longhand_properties::DeclarationBlockNoRedundantLonghandProperties,
  declaration_block_no_shorthand_property_overrides::DeclarationBlockNoShorthandPropertyOverrides,
  declaration_block_single_line_max_declarations::DeclarationBlockSingleLineMaxDeclarations,
  declaration_empty_line_before::DeclarationEmptyLineBefore,
  declaration_no_important::DeclarationNoImportant,
  declaration_property_unit_allowed_list::DeclarationPropertyUnitAllowedList,
  declaration_property_unit_disallowed_list::DeclarationPropertyUnitDisallowedList,
  declaration_property_max_values::DeclarationPropertyMaxValues,
  declaration_property_value_allowed_list::DeclarationPropertyValueAllowedList,
  declaration_property_value_disallowed_list::DeclarationPropertyValueDisallowedList,
  declaration_property_value_keyword_no_deprecated::DeclarationPropertyValueKeywordNoDeprecated,
  declaration_property_value_no_unknown::DeclarationPropertyValueNoUnknown,
  display_notation::DisplayNotation,
  font_family_name_quotes::FontFamilyNameQuotes,
  font_family_no_duplicate_names::FontFamilyNoDuplicateNames,
  font_family_no_missing_generic_family_keyword::FontFamilyNoMissingGenericFamilyKeyword,
  font_weight_notation::FontWeightNotation,
  function_allowed_list::FunctionAllowedList,
  function_calc_no_unspaced_operator::FunctionCalcNoUnspacedOperator,
  function_disallowed_list::FunctionDisallowedList,
  function_linear_gradient_no_nonstandard_direction::FunctionLinearGradientNoNonstandardDirection,
  function_no_unknown::FunctionNoUnknown,
  function_name_case::FunctionNameCase,
  function_url_no_scheme_relative::FunctionUrlNoSchemeRelative,
  function_url_quotes::FunctionUrlQuotes,
  function_url_scheme_allowed_list::FunctionUrlSchemeAllowedList,
  function_url_scheme_disallowed_list::FunctionUrlSchemeDisallowedList,
  hue_degree_notation::HueDegreeNotation,
  import_notation::ImportNotation,
  keyframe_block_no_duplicate_selectors::KeyframeBlockNoDuplicateSelectors,
  keyframe_selector_notation::KeyframeSelectorNotation,
  keyframes_name_pattern::KeyframesNamePattern,
  keyframe_declaration_no_important::KeyframeDeclarationNoImportant,
  layer_name_pattern::LayerNamePattern,
  length_zero_no_unit::LengthZeroNoUnit,
  lightness_notation::LightnessNotation,
  max_line_length::MaxLineLength,
  max_nesting_depth::MaxNestingDepth,
  media_feature_name_allowed_list::MediaFeatureNameAllowedList,
  media_feature_name_disallowed_list::MediaFeatureNameDisallowedList,
  media_feature_name_no_unknown::MediaFeatureNameNoUnknown,
  media_feature_name_no_vendor_prefix::MediaFeatureNameNoVendorPrefix,
  media_feature_name_unit_allowed_list::MediaFeatureNameUnitAllowedList,
  media_feature_name_value_allowed_list::MediaFeatureNameValueAllowedList,
  media_feature_name_value_no_unknown::MediaFeatureNameValueNoUnknown,
  media_feature_range_notation::MediaFeatureRangeNotation,
  media_query_no_invalid::MediaQueryNoInvalid,
  material_no_prefixes::MaterialNoPrefixes,
  media_type_no_deprecated::MediaTypeNoDeprecated,
  named_grid_areas_no_invalid::NamedGridAreasNoInvalid,
  nesting_selector_no_missing_scoping_root::NestingSelectorNoMissingScopingRoot,
  no_descending_specificity::NoDescendingSpecificity,
  no_duplicate_at_import_rules::NoDuplicateAtImportRules,
  no_duplicate_selectors::NoDuplicateSelectors,
  no_empty_source::NoEmptySource,
  no_invalid_double_slash_comments::NoInvalidDoubleSlashComments,
  no_invalid_position_at_import_rule::NoInvalidPositionAtImportRule,
  no_invalid_position_declaration::NoInvalidPositionDeclaration,
  no_irregular_whitespace::NoIrregularWhitespace,
  no_unknown_animations::NoUnknownAnimations,
  // NOTE: `number-leading-zero` is NOT registered here.  The deprecated
  // name resolves via `resolve_deprecated_alias` to the canonical
  // `@stylistic/number-leading-zero` rule which is registered below.
  // Registering the standalone rule would shadow the alias and bypass
  // the correct source-level implementation (causing wrong span offsets).
  order_order::OrderOrder,
  order_properties_alphabetical_order::OrderPropertiesAlphabeticalOrder,
  order_properties_order::OrderPropertiesOrder,
  plugin_browser_compat::PluginBrowserCompat,
  plugin_enforce_variable_for_property::PluginEnforceVariableForProperty,
  plugin_no_unknown_custom_properties::PluginNoUnknownCustomProperties,
  plugin_no_unused_custom_properties::PluginNoUnusedCustomProperties,
  plugin_require_file_header_comment::PluginRequireFileHeaderComment,
  number_max_precision::NumberMaxPrecision,
  property_allowed_list::PropertyAllowedList,
  property_disallowed_list::PropertyDisallowedList,
  property_no_deprecated::PropertyNoDeprecated,
  property_no_unknown::PropertyNoUnknown,
  property_no_vendor_prefix::PropertyNoVendorPrefix,
  rule_empty_line_before::RuleEmptyLineBefore,
  rule_nesting_at_rule_required_list::RuleNestingAtRuleRequiredList,
  rule_selector_property_disallowed_list::RuleSelectorPropertyDisallowedList,
  selector_anb_no_unmatchable::SelectorAnbNoUnmatchable,
  selector_attribute_name_disallowed_list::SelectorAttributeNameDisallowedList,
  selector_attribute_operator_allowed_list::SelectorAttributeOperatorAllowedList,
  selector_attribute_operator_disallowed_list::SelectorAttributeOperatorDisallowedList,
  selector_attribute_quotes::SelectorAttributeQuotes,
  selector_class_pattern::SelectorClassPattern,
  selector_combinator_allowed_list::SelectorCombinatorAllowedList,
  selector_combinator_disallowed_list::SelectorCombinatorDisallowedList,
  selector_disallowed_list::SelectorDisallowedList,
  selector_id_pattern::SelectorIdPattern,
  selector_max_attribute::SelectorMaxAttribute,
  selector_max_class::SelectorMaxClass,
  selector_max_combinators::SelectorMaxCombinators,
  selector_max_compound_selectors::SelectorMaxCompoundSelectors,
  selector_max_id::SelectorMaxId,
  selector_max_pseudo_class::SelectorMaxPseudoClass,
  selector_max_specificity::SelectorMaxSpecificity,
  selector_max_type::SelectorMaxType,
  selector_max_universal::SelectorMaxUniversal,
  selector_nested_pattern::SelectorNestedPattern,
  selector_no_qualifying_type::SelectorNoQualifyingType,
  selector_no_invalid::SelectorNoInvalid,
  selector_no_unmatchable::SelectorNoUnmatchable,
  selector_no_vendor_prefix::SelectorNoVendorPrefix,
  selector_type_case::SelectorTypeCase,
  selector_not_notation::SelectorNotNotation,
  selector_pseudo_class_allowed_list::SelectorPseudoClassAllowedList,
  selector_pseudo_class_disallowed_list::SelectorPseudoClassDisallowedList,
  selector_pseudo_class_no_unknown::SelectorPseudoClassNoUnknown,
  selector_pseudo_element_allowed_list::SelectorPseudoElementAllowedList,
  selector_pseudo_element_colon_notation::SelectorPseudoElementColonNotation,
  selector_pseudo_element_disallowed_list::SelectorPseudoElementDisallowedList,
  selector_pseudo_element_no_unknown::SelectorPseudoElementNoUnknown,
  selector_type_no_unknown::SelectorTypeNoUnknown,
  shorthand_property_no_redundant_values::ShorthandPropertyNoRedundantValues,
  string_no_newline::StringNoNewline,
  string_quotes::StringQuotes,
  syntax_string_no_invalid::SyntaxStringNoInvalid,
  time_min_milliseconds::TimeMinMilliseconds,
  unit_allowed_list::UnitAllowedList,
  unit_disallowed_list::UnitDisallowedList,
  unit_no_unknown::UnitNoUnknown,
  value_keyword_case::ValueKeywordCase,
  value_no_vendor_prefix::ValueNoVendorPrefix,
  // Spectrum tools custom plugin rules
  spectrum_tools_no_unknown_custom_properties::SpectrumToolsNoUnknownCustomProperties,
  // @stylistic rules
  stylistic_block_closing_brace_newline_after::StylisticBlockClosingBraceNewlineAfter,
  stylistic_at_rule_name_space_after::StylisticAtRuleNameSpaceAfter,
  stylistic_at_rule_semicolon_newline_after::StylisticAtRuleSemicolonNewlineAfter,
  stylistic_selector_combinator_space_before::StylisticSelectorCombinatorSpaceBefore,
  stylistic_selector_pseudo_element_case::StylisticSelectorPseudoElementCase,
  stylistic_media_feature_colon_space_before::StylisticMediaFeatureColonSpaceBefore,
  stylistic_media_query_list_comma_newline_after::StylisticMediaQueryListCommaNewlineAfter,
  stylistic_media_query_list_comma_space_before::StylisticMediaQueryListCommaSpaceBefore,
  stylistic_media_query_list_comma_space_after::StylisticMediaQueryListCommaSpaceAfter,
  stylistic_media_feature_range_operator_space_after::StylisticMediaFeatureRangeOperatorSpaceAfter,
  stylistic_max_empty_lines::StylisticMaxEmptyLines,
  stylistic_value_list_comma_newline_after::StylisticValueListCommaNewlineAfter,
  stylistic_declaration_colon_space_after::StylisticDeclarationColonSpaceAfter,
  stylistic_declaration_colon_space_before::StylisticDeclarationColonSpaceBefore,
  stylistic_declaration_bang_space_before::StylisticDeclarationBangSpaceBefore,
  stylistic_declaration_bang_space_after::StylisticDeclarationBangSpaceAfter,
  stylistic_function_comma_space_after::StylisticFunctionCommaSpaceAfter,
  stylistic_function_comma_space_before::StylisticFunctionCommaSpaceBefore,
  stylistic_function_parentheses_newline_inside::StylisticFunctionParenthesesNewlineInside,
  stylistic_function_parentheses_space_inside::StylisticFunctionParenthesesSpaceInside,
  stylistic_function_whitespace_after::StylisticFunctionWhitespaceAfter,
  stylistic_string_quotes::StylisticStringQuotes,
  stylistic_value_list_comma_space_after::StylisticValueListCommaSpaceAfter,
  stylistic_color_hex_case::StylisticColorHexCase,
  stylistic_declaration_block_semicolon_newline_before::StylisticDeclarationBlockSemicolonNewlineBefore,
  stylistic_declaration_block_semicolon_space_after::StylisticDeclarationBlockSemicolonSpaceAfter,
  stylistic_declaration_block_semicolon_space_before::StylisticDeclarationBlockSemicolonSpaceBefore,
  stylistic_declaration_block_trailing_semicolon::StylisticDeclarationBlockTrailingSemicolon,
  stylistic_no_missing_end_of_source_newline::StylisticNoMissingEndOfSourceNewline,
  stylistic_number_no_trailing_zeros::StylisticNumberNoTrailingZeros,
  stylistic_property_case::StylisticPropertyCase,
  stylistic_selector_attribute_operator_space_before::StylisticSelectorAttributeOperatorSpaceBefore,
  stylistic_selector_list_comma_newline_after::StylisticSelectorListCommaNewlineAfter,
  stylistic_selector_list_comma_newline_before::StylisticSelectorListCommaNewlineBefore,
  stylistic_selector_list_comma_space_after::StylisticSelectorListCommaSpaceAfter,
  stylistic_selector_list_comma_space_before::StylisticSelectorListCommaSpaceBefore,
  stylistic_selector_max_empty_lines::StylisticSelectorMaxEmptyLines,
  stylistic_selector_pseudo_class_parentheses_space_inside::StylisticSelectorPseudoClassParenthesesSpaceInside,
  stylistic_function_max_empty_lines::StylisticFunctionMaxEmptyLines,
  stylistic_value_list_comma_newline_before::StylisticValueListCommaNewlineBefore,
  stylistic_media_feature_range_operator_space_before::StylisticMediaFeatureRangeOperatorSpaceBefore,
  stylistic_media_feature_parentheses_space_inside::StylisticMediaFeatureParenthesesSpaceInside,
  stylistic_block_closing_brace_empty_line_before::StylisticBlockClosingBraceEmptyLineBefore,
  stylistic_block_closing_brace_newline_before::StylisticBlockClosingBraceNewlineBefore,
  stylistic_unicode_bom::StylisticUnicodeBom,
  stylistic_unit_case::StylisticUnitCase,
  stylistic_indentation::StylisticIndentation,
  stylistic_no_eol_whitespace::StylisticNoEolWhitespace,
  stylistic_no_extra_semicolons::StylisticNoExtraSemicolons,
  stylistic_number_leading_zero::StylisticNumberLeadingZero,
  stylistic_declaration_block_semicolon_newline_after::StylisticDeclarationBlockSemicolonNewlineAfter,
  stylistic_declaration_colon_newline_after::StylisticDeclarationColonNewlineAfter,
  stylistic_selector_combinator_space_after::StylisticSelectorCombinatorSpaceAfter,
  stylistic_selector_attribute_operator_space_after::StylisticSelectorAttributeOperatorSpaceAfter,
  stylistic_block_opening_brace_space_before::StylisticBlockOpeningBraceSpaceBefore,
  stylistic_at_rule_name_case::StylisticAtRuleNameCase,
  stylistic_at_rule_semicolon_space_before::StylisticAtRuleSemicolonSpaceBefore,
  stylistic_block_opening_brace_newline_after::StylisticBlockOpeningBraceNewlineAfter,
  stylistic_media_feature_colon_space_after::StylisticMediaFeatureColonSpaceAfter,
  stylistic_selector_attribute_brackets_space_inside::StylisticSelectorAttributeBracketsSpaceInside,
  stylistic_selector_pseudo_class_case::StylisticSelectorPseudoClassCase,
  stylistic_value_list_comma_space_before::StylisticValueListCommaSpaceBefore,
  stylistic_value_list_max_empty_lines::StylisticValueListMaxEmptyLines,
  stylistic_selector_descendant_combinator_no_non_space::StylisticSelectorDescendantCombinatorNoNonSpace,
  stylistic_block_opening_brace_space_after::StylisticBlockOpeningBraceSpaceAfter,
  stylistic_block_closing_brace_space_before::StylisticBlockClosingBraceSpaceBefore,
  stylistic_no_empty_first_line::StylisticNoEmptyFirstLine,
  stylistic_media_feature_name_case::StylisticMediaFeatureNameCase,
  stylistic_function_comma_newline_after::StylisticFunctionCommaNewlineAfter,
  // SCSS-specific rules
  scss_at_else_closing_brace_newline_after::ScssAtElseClosingBraceNewlineAfter,
  scss_at_else_closing_brace_space_after::ScssAtElseClosingBraceSpaceAfter,
  scss_at_extend_no_missing_placeholder::ScssAtExtendNoMissingPlaceholder,
  scss_at_function_pattern::ScssAtFunctionPattern,
  scss_at_if_closing_brace_newline_after::ScssAtIfClosingBraceNewlineAfter,
  scss_at_if_closing_brace_space_after::ScssAtIfClosingBraceSpaceAfter,
  scss_at_if_no_null::ScssAtIfNoNull,
  scss_at_import_partial_extension::ScssAtImportPartialExtension,
  scss_at_import_partial_extension_disallowed_list::ScssAtImportPartialExtensionDisallowedList,
  scss_at_mixin_argumentless_call_parentheses::ScssAtMixinArgumentlessCallParentheses,
  scss_at_mixin_disallowed_list::ScssAtMixinDisallowedList,
  scss_at_mixin_pattern::ScssAtMixinPattern,
  scss_at_rule_no_unknown::ScssAtRuleNoUnknown,
  scss_comment_no_empty::ScssCommentNoEmpty,
  scss_declaration_nested_properties::ScssDeclarationNestedProperties,
  scss_declaration_nested_properties_no_divided_groups::ScssDeclarationNestedPropertiesNoDividedGroups,
  scss_dollar_variable_colon_space_after::ScssDollarVariableColonSpaceAfter,
  scss_dollar_variable_colon_space_before::ScssDollarVariableColonSpaceBefore,
  scss_dollar_variable_no_missing_interpolation::ScssDollarVariableNoMissingInterpolation,
  scss_dollar_variable_pattern::ScssDollarVariablePattern,
  scss_double_slash_comment_whitespace_inside::ScssDoubleSlashCommentWhitespaceInside,
  scss_function_no_unknown::ScssFunctionNoUnknown,
  scss_function_quote_no_quoted_strings_inside::ScssFunctionQuoteNoQuotedStringsInside,
  scss_function_unquote_no_unquoted_strings_inside::ScssFunctionUnquoteNoUnquotedStringsInside,
  scss_load_no_partial_leading_underscore::ScssLoadNoPartialLeadingUnderscore,
  scss_load_partial_extension::ScssLoadPartialExtension,
  scss_no_duplicate_dollar_variables::ScssNoDuplicateDollarVariables,
  scss_no_duplicate_mixins::ScssNoDuplicateMixins,
  scss_no_global_function_names::ScssNoGlobalFunctionNames,
  scss_operator_no_newline_after::ScssOperatorNoNewlineAfter,
  scss_operator_no_newline_before::ScssOperatorNoNewlineBefore,
  scss_operator_no_unspaced::ScssOperatorNoUnspaced,
  scss_partial_no_import::ScssPartialNoImport,
  scss_percent_placeholder_pattern::ScssPercentPlaceholderPattern,
  scss_selector_no_redundant_nesting_selector::ScssSelectorNoRedundantNestingSelector,
  scss_dollar_variable_empty_line_before::ScssDollarVariableEmptyLineBefore,
  scss_comment_no_loud::ScssCommentNoLoud,
  scss_at_mixin_parentheses_space_before::ScssAtMixinParenthesesSpaceBefore,
  scss_at_function_parentheses_space_before::ScssAtFunctionParenthesesSpaceBefore,
  scss_at_else_if_parentheses_space_before::ScssAtElseIfParenthesesSpaceBefore,
  scss_at_else_empty_line_before::ScssAtElseEmptyLineBefore,
  scss_double_slash_comment_inline::ScssDoubleSlashCommentInline,
  scss_double_slash_comment_empty_line_before::ScssDoubleSlashCommentEmptyLineBefore,
  scss_at_rule_conditional_no_parentheses::ScssAtRuleConditionalNoParentheses,
  scss_function_disallowed_list::ScssFunctionDisallowedList,
}
