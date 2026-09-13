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
pub mod selector_no_qualifying_type;
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

/// Register all built-in rules in the given registry.
pub fn register_all(registry: &mut RuleRegistry) {
  registry.register(Box::new(alpha_value_notation::AlphaValueNotation));
  registry.register(Box::new(annotation_no_unknown::AnnotationNoUnknown));
  registry.register(Box::new(at_rule_allowed_list::AtRuleAllowedList));
  registry.register(Box::new(
    at_rule_descriptor_no_unknown::AtRuleDescriptorNoUnknown,
  ));
  registry.register(Box::new(
    at_rule_descriptor_value_no_unknown::AtRuleDescriptorValueNoUnknown,
  ));
  registry.register(Box::new(at_rule_disallowed_list::AtRuleDisallowedList));
  registry.register(Box::new(at_rule_empty_line_before::AtRuleEmptyLineBefore));
  registry.register(Box::new(at_rule_no_deprecated::AtRuleNoDeprecated));
  registry.register(Box::new(at_rule_no_unknown::AtRuleNoUnknown));
  registry.register(Box::new(at_rule_no_vendor_prefix::AtRuleNoVendorPrefix));
  registry.register(Box::new(at_rule_prelude_no_invalid::AtRulePreludeNoInvalid));
  registry.register(Box::new(
    at_rule_property_required_list::AtRulePropertyRequiredList,
  ));
  registry.register(Box::new(block_no_empty::BlockNoEmpty));
  registry.register(Box::new(
    block_no_redundant_nested_style_rules::BlockNoRedundantNestedStyleRules,
  ));
  registry.register(Box::new(
    color_function_alias_notation::ColorFunctionAliasNotation,
  ));
  registry.register(Box::new(color_function_notation::ColorFunctionNotation));
  registry.register(Box::new(color_hex_alpha::ColorHexAlpha));
  registry.register(Box::new(color_hex_case::ColorHexCase));
  registry.register(Box::new(color_hex_length::ColorHexLength));
  registry.register(Box::new(color_named::ColorNamed));
  registry.register(Box::new(color_no_hex::ColorNoHex));
  registry.register(Box::new(color_no_invalid_hex::ColorNoInvalidHex));
  registry.register(Box::new(comment_empty_line_before::CommentEmptyLineBefore));
  registry.register(Box::new(
    csstools_value_no_unknown_custom_properties::CsstoolsValueNoUnknownCustomProperties,
  ));
  registry.register(Box::new(comment_no_empty::CommentNoEmpty));
  registry.register(Box::new(comment_pattern::CommentPattern));
  registry.register(Box::new(comment_whitespace_inside::CommentWhitespaceInside));
  registry.register(Box::new(
    comment_word_disallowed_list::CommentWordDisallowedList,
  ));
  registry.register(Box::new(container_name_pattern::ContainerNamePattern));
  registry.register(Box::new(custom_media_pattern::CustomMediaPattern));
  registry.register(Box::new(
    custom_property_empty_line_before::CustomPropertyEmptyLineBefore,
  ));
  registry.register(Box::new(
    custom_property_no_missing_var_function::CustomPropertyNoMissingVarFunction,
  ));
  registry.register(Box::new(custom_property_pattern::CustomPropertyPattern));
  registry.register(Box::new(
    declaration_block_no_duplicate_custom_properties::DeclarationBlockNoDuplicateCustomProperties,
  ));
  registry.register(Box::new(
    declaration_block_no_duplicate_properties::DeclarationBlockNoDuplicateProperties,
  ));
  registry.register(Box::new(declaration_block_no_redundant_longhand_properties::DeclarationBlockNoRedundantLonghandProperties));
  registry.register(Box::new(
    declaration_block_no_shorthand_property_overrides::DeclarationBlockNoShorthandPropertyOverrides,
  ));
  registry.register(Box::new(
    declaration_block_single_line_max_declarations::DeclarationBlockSingleLineMaxDeclarations,
  ));
  registry.register(Box::new(
    declaration_empty_line_before::DeclarationEmptyLineBefore,
  ));
  registry.register(Box::new(declaration_no_important::DeclarationNoImportant));
  registry.register(Box::new(
    declaration_property_unit_allowed_list::DeclarationPropertyUnitAllowedList,
  ));
  registry.register(Box::new(
    declaration_property_unit_disallowed_list::DeclarationPropertyUnitDisallowedList,
  ));
  registry.register(Box::new(
    declaration_property_max_values::DeclarationPropertyMaxValues,
  ));
  registry.register(Box::new(
    declaration_property_value_allowed_list::DeclarationPropertyValueAllowedList,
  ));
  registry.register(Box::new(
    declaration_property_value_disallowed_list::DeclarationPropertyValueDisallowedList,
  ));
  registry.register(Box::new(
    declaration_property_value_keyword_no_deprecated::DeclarationPropertyValueKeywordNoDeprecated,
  ));
  registry.register(Box::new(
    declaration_property_value_no_unknown::DeclarationPropertyValueNoUnknown,
  ));
  registry.register(Box::new(display_notation::DisplayNotation));
  registry.register(Box::new(font_family_name_quotes::FontFamilyNameQuotes));
  registry.register(Box::new(
    font_family_no_duplicate_names::FontFamilyNoDuplicateNames,
  ));
  registry.register(Box::new(
    font_family_no_missing_generic_family_keyword::FontFamilyNoMissingGenericFamilyKeyword,
  ));
  registry.register(Box::new(font_weight_notation::FontWeightNotation));
  registry.register(Box::new(function_allowed_list::FunctionAllowedList));
  registry.register(Box::new(
    function_calc_no_unspaced_operator::FunctionCalcNoUnspacedOperator,
  ));
  registry.register(Box::new(function_disallowed_list::FunctionDisallowedList));
  registry.register(Box::new(
    function_linear_gradient_no_nonstandard_direction::FunctionLinearGradientNoNonstandardDirection,
  ));
  registry.register(Box::new(function_no_unknown::FunctionNoUnknown));
  registry.register(Box::new(function_name_case::FunctionNameCase));
  registry.register(Box::new(
    function_url_no_scheme_relative::FunctionUrlNoSchemeRelative,
  ));
  registry.register(Box::new(function_url_quotes::FunctionUrlQuotes));
  registry.register(Box::new(
    function_url_scheme_allowed_list::FunctionUrlSchemeAllowedList,
  ));
  registry.register(Box::new(
    function_url_scheme_disallowed_list::FunctionUrlSchemeDisallowedList,
  ));
  registry.register(Box::new(hue_degree_notation::HueDegreeNotation));
  registry.register(Box::new(import_notation::ImportNotation));
  registry.register(Box::new(
    keyframe_block_no_duplicate_selectors::KeyframeBlockNoDuplicateSelectors,
  ));
  registry.register(Box::new(
    keyframe_selector_notation::KeyframeSelectorNotation,
  ));
  registry.register(Box::new(keyframes_name_pattern::KeyframesNamePattern));
  registry.register(Box::new(
    keyframe_declaration_no_important::KeyframeDeclarationNoImportant,
  ));
  registry.register(Box::new(layer_name_pattern::LayerNamePattern));
  registry.register(Box::new(length_zero_no_unit::LengthZeroNoUnit));
  registry.register(Box::new(lightness_notation::LightnessNotation));
  registry.register(Box::new(max_line_length::MaxLineLength));
  registry.register(Box::new(max_nesting_depth::MaxNestingDepth));
  registry.register(Box::new(
    media_feature_name_allowed_list::MediaFeatureNameAllowedList,
  ));
  registry.register(Box::new(
    media_feature_name_disallowed_list::MediaFeatureNameDisallowedList,
  ));
  registry.register(Box::new(
    media_feature_name_no_unknown::MediaFeatureNameNoUnknown,
  ));
  registry.register(Box::new(
    media_feature_name_no_vendor_prefix::MediaFeatureNameNoVendorPrefix,
  ));
  registry.register(Box::new(
    media_feature_name_unit_allowed_list::MediaFeatureNameUnitAllowedList,
  ));
  registry.register(Box::new(
    media_feature_name_value_allowed_list::MediaFeatureNameValueAllowedList,
  ));
  registry.register(Box::new(
    media_feature_name_value_no_unknown::MediaFeatureNameValueNoUnknown,
  ));
  registry.register(Box::new(
    media_feature_range_notation::MediaFeatureRangeNotation,
  ));
  registry.register(Box::new(media_query_no_invalid::MediaQueryNoInvalid));
  registry.register(Box::new(material_no_prefixes::MaterialNoPrefixes));
  registry.register(Box::new(media_type_no_deprecated::MediaTypeNoDeprecated));
  registry.register(Box::new(
    named_grid_areas_no_invalid::NamedGridAreasNoInvalid,
  ));
  registry.register(Box::new(
    nesting_selector_no_missing_scoping_root::NestingSelectorNoMissingScopingRoot,
  ));
  registry.register(Box::new(no_descending_specificity::NoDescendingSpecificity));
  registry.register(Box::new(
    no_duplicate_at_import_rules::NoDuplicateAtImportRules,
  ));
  registry.register(Box::new(no_duplicate_selectors::NoDuplicateSelectors));
  registry.register(Box::new(no_empty_source::NoEmptySource));
  registry.register(Box::new(
    no_invalid_double_slash_comments::NoInvalidDoubleSlashComments,
  ));
  registry.register(Box::new(
    no_invalid_position_at_import_rule::NoInvalidPositionAtImportRule,
  ));
  registry.register(Box::new(
    no_invalid_position_declaration::NoInvalidPositionDeclaration,
  ));
  registry.register(Box::new(no_irregular_whitespace::NoIrregularWhitespace));
  registry.register(Box::new(no_unknown_animations::NoUnknownAnimations));
  // NOTE: `number-leading-zero` is NOT registered here.  The deprecated
  // name resolves via `resolve_deprecated_alias` to the canonical
  // `@stylistic/number-leading-zero` rule which is registered below.
  // Registering the standalone rule would shadow the alias and bypass
  // the correct source-level implementation (causing wrong span offsets).
  registry.register(Box::new(order_order::OrderOrder));
  registry.register(Box::new(
    order_properties_alphabetical_order::OrderPropertiesAlphabeticalOrder,
  ));
  registry.register(Box::new(order_properties_order::OrderPropertiesOrder));
  registry.register(Box::new(plugin_browser_compat::PluginBrowserCompat));
  registry.register(Box::new(
    plugin_enforce_variable_for_property::PluginEnforceVariableForProperty,
  ));
  registry.register(Box::new(
    plugin_no_unknown_custom_properties::PluginNoUnknownCustomProperties,
  ));
  registry.register(Box::new(
    plugin_no_unused_custom_properties::PluginNoUnusedCustomProperties,
  ));
  registry.register(Box::new(
    plugin_require_file_header_comment::PluginRequireFileHeaderComment,
  ));
  registry.register(Box::new(number_max_precision::NumberMaxPrecision));
  registry.register(Box::new(property_allowed_list::PropertyAllowedList));
  registry.register(Box::new(property_disallowed_list::PropertyDisallowedList));
  registry.register(Box::new(property_no_deprecated::PropertyNoDeprecated));
  registry.register(Box::new(property_no_unknown::PropertyNoUnknown));
  registry.register(Box::new(property_no_vendor_prefix::PropertyNoVendorPrefix));
  registry.register(Box::new(rule_empty_line_before::RuleEmptyLineBefore));
  registry.register(Box::new(
    rule_nesting_at_rule_required_list::RuleNestingAtRuleRequiredList,
  ));
  registry.register(Box::new(
    rule_selector_property_disallowed_list::RuleSelectorPropertyDisallowedList,
  ));
  registry.register(Box::new(
    selector_anb_no_unmatchable::SelectorAnbNoUnmatchable,
  ));
  registry.register(Box::new(
    selector_attribute_name_disallowed_list::SelectorAttributeNameDisallowedList,
  ));
  registry.register(Box::new(
    selector_attribute_operator_allowed_list::SelectorAttributeOperatorAllowedList,
  ));
  registry.register(Box::new(
    selector_attribute_operator_disallowed_list::SelectorAttributeOperatorDisallowedList,
  ));
  registry.register(Box::new(selector_attribute_quotes::SelectorAttributeQuotes));
  registry.register(Box::new(selector_class_pattern::SelectorClassPattern));
  registry.register(Box::new(
    selector_combinator_allowed_list::SelectorCombinatorAllowedList,
  ));
  registry.register(Box::new(
    selector_combinator_disallowed_list::SelectorCombinatorDisallowedList,
  ));
  registry.register(Box::new(selector_disallowed_list::SelectorDisallowedList));
  registry.register(Box::new(selector_id_pattern::SelectorIdPattern));
  registry.register(Box::new(selector_max_attribute::SelectorMaxAttribute));
  registry.register(Box::new(selector_max_class::SelectorMaxClass));
  registry.register(Box::new(selector_max_combinators::SelectorMaxCombinators));
  registry.register(Box::new(
    selector_max_compound_selectors::SelectorMaxCompoundSelectors,
  ));
  registry.register(Box::new(selector_max_id::SelectorMaxId));
  registry.register(Box::new(selector_max_pseudo_class::SelectorMaxPseudoClass));
  registry.register(Box::new(selector_max_specificity::SelectorMaxSpecificity));
  registry.register(Box::new(selector_max_type::SelectorMaxType));
  registry.register(Box::new(selector_max_universal::SelectorMaxUniversal));
  registry.register(Box::new(selector_nested_pattern::SelectorNestedPattern));
  registry.register(Box::new(
    selector_no_qualifying_type::SelectorNoQualifyingType,
  ));
  registry.register(Box::new(selector_no_vendor_prefix::SelectorNoVendorPrefix));
  registry.register(Box::new(selector_type_case::SelectorTypeCase));
  registry.register(Box::new(selector_not_notation::SelectorNotNotation));
  registry.register(Box::new(
    selector_pseudo_class_allowed_list::SelectorPseudoClassAllowedList,
  ));
  registry.register(Box::new(
    selector_pseudo_class_disallowed_list::SelectorPseudoClassDisallowedList,
  ));
  registry.register(Box::new(
    selector_pseudo_class_no_unknown::SelectorPseudoClassNoUnknown,
  ));
  registry.register(Box::new(
    selector_pseudo_element_allowed_list::SelectorPseudoElementAllowedList,
  ));
  registry.register(Box::new(
    selector_pseudo_element_colon_notation::SelectorPseudoElementColonNotation,
  ));
  registry.register(Box::new(
    selector_pseudo_element_disallowed_list::SelectorPseudoElementDisallowedList,
  ));
  registry.register(Box::new(
    selector_pseudo_element_no_unknown::SelectorPseudoElementNoUnknown,
  ));
  registry.register(Box::new(selector_type_no_unknown::SelectorTypeNoUnknown));
  registry.register(Box::new(
    shorthand_property_no_redundant_values::ShorthandPropertyNoRedundantValues,
  ));
  registry.register(Box::new(string_no_newline::StringNoNewline));
  registry.register(Box::new(string_quotes::StringQuotes));
  registry.register(Box::new(syntax_string_no_invalid::SyntaxStringNoInvalid));
  registry.register(Box::new(time_min_milliseconds::TimeMinMilliseconds));
  registry.register(Box::new(unit_allowed_list::UnitAllowedList));
  registry.register(Box::new(unit_disallowed_list::UnitDisallowedList));
  registry.register(Box::new(unit_no_unknown::UnitNoUnknown));
  registry.register(Box::new(value_keyword_case::ValueKeywordCase));
  registry.register(Box::new(value_no_vendor_prefix::ValueNoVendorPrefix));

  // Spectrum tools custom plugin rules
  registry.register(Box::new(
    spectrum_tools_no_unknown_custom_properties::SpectrumToolsNoUnknownCustomProperties,
  ));

  // @stylistic rules
  registry.register(Box::new(
    stylistic_block_closing_brace_newline_after::StylisticBlockClosingBraceNewlineAfter,
  ));
  registry.register(Box::new(
    stylistic_at_rule_name_space_after::StylisticAtRuleNameSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_at_rule_semicolon_newline_after::StylisticAtRuleSemicolonNewlineAfter,
  ));
  registry.register(Box::new(
    stylistic_selector_combinator_space_before::StylisticSelectorCombinatorSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_selector_pseudo_element_case::StylisticSelectorPseudoElementCase,
  ));
  registry.register(Box::new(
    stylistic_media_feature_colon_space_before::StylisticMediaFeatureColonSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_media_query_list_comma_newline_after::StylisticMediaQueryListCommaNewlineAfter,
  ));
  registry.register(Box::new(
    stylistic_media_query_list_comma_space_before::StylisticMediaQueryListCommaSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_media_query_list_comma_space_after::StylisticMediaQueryListCommaSpaceAfter,
  ));
  registry.register(Box::new(
        stylistic_media_feature_range_operator_space_after::StylisticMediaFeatureRangeOperatorSpaceAfter,
    ));
  registry.register(Box::new(stylistic_max_empty_lines::StylisticMaxEmptyLines));
  registry.register(Box::new(
    stylistic_value_list_comma_newline_after::StylisticValueListCommaNewlineAfter,
  ));
  registry.register(Box::new(
    stylistic_declaration_colon_space_after::StylisticDeclarationColonSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_declaration_colon_space_before::StylisticDeclarationColonSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_declaration_bang_space_before::StylisticDeclarationBangSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_declaration_bang_space_after::StylisticDeclarationBangSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_function_comma_space_after::StylisticFunctionCommaSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_function_comma_space_before::StylisticFunctionCommaSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_function_parentheses_newline_inside::StylisticFunctionParenthesesNewlineInside,
  ));
  registry.register(Box::new(
    stylistic_function_parentheses_space_inside::StylisticFunctionParenthesesSpaceInside,
  ));
  registry.register(Box::new(
    stylistic_function_whitespace_after::StylisticFunctionWhitespaceAfter,
  ));
  registry.register(Box::new(stylistic_string_quotes::StylisticStringQuotes));
  registry.register(Box::new(
    stylistic_value_list_comma_space_after::StylisticValueListCommaSpaceAfter,
  ));
  registry.register(Box::new(stylistic_color_hex_case::StylisticColorHexCase));
  registry.register(Box::new(
        stylistic_declaration_block_semicolon_newline_before::StylisticDeclarationBlockSemicolonNewlineBefore,
    ));
  registry.register(Box::new(
    stylistic_declaration_block_semicolon_space_after::StylisticDeclarationBlockSemicolonSpaceAfter,
  ));
  registry.register(Box::new(
        stylistic_declaration_block_semicolon_space_before::StylisticDeclarationBlockSemicolonSpaceBefore,
    ));
  registry.register(Box::new(
    stylistic_declaration_block_trailing_semicolon::StylisticDeclarationBlockTrailingSemicolon,
  ));
  registry.register(Box::new(
    stylistic_no_missing_end_of_source_newline::StylisticNoMissingEndOfSourceNewline,
  ));
  registry.register(Box::new(
    stylistic_number_no_trailing_zeros::StylisticNumberNoTrailingZeros,
  ));
  registry.register(Box::new(stylistic_property_case::StylisticPropertyCase));
  registry.register(Box::new(
        stylistic_selector_attribute_operator_space_before::StylisticSelectorAttributeOperatorSpaceBefore,
    ));
  registry.register(Box::new(
    stylistic_selector_list_comma_newline_after::StylisticSelectorListCommaNewlineAfter,
  ));
  registry.register(Box::new(
    stylistic_selector_list_comma_newline_before::StylisticSelectorListCommaNewlineBefore,
  ));
  registry.register(Box::new(
    stylistic_selector_list_comma_space_after::StylisticSelectorListCommaSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_selector_list_comma_space_before::StylisticSelectorListCommaSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_selector_max_empty_lines::StylisticSelectorMaxEmptyLines,
  ));
  registry.register(Box::new(
        stylistic_selector_pseudo_class_parentheses_space_inside::StylisticSelectorPseudoClassParenthesesSpaceInside,
    ));
  registry.register(Box::new(
    stylistic_function_max_empty_lines::StylisticFunctionMaxEmptyLines,
  ));
  registry.register(Box::new(
    stylistic_value_list_comma_newline_before::StylisticValueListCommaNewlineBefore,
  ));
  registry.register(Box::new(
        stylistic_media_feature_range_operator_space_before::StylisticMediaFeatureRangeOperatorSpaceBefore,
    ));
  registry.register(Box::new(
    stylistic_media_feature_parentheses_space_inside::StylisticMediaFeatureParenthesesSpaceInside,
  ));
  registry.register(Box::new(
    stylistic_block_closing_brace_empty_line_before::StylisticBlockClosingBraceEmptyLineBefore,
  ));
  registry.register(Box::new(
    stylistic_block_closing_brace_newline_before::StylisticBlockClosingBraceNewlineBefore,
  ));
  registry.register(Box::new(stylistic_unicode_bom::StylisticUnicodeBom));
  registry.register(Box::new(stylistic_unit_case::StylisticUnitCase));
  registry.register(Box::new(stylistic_indentation::StylisticIndentation));
  registry.register(Box::new(
    stylistic_no_eol_whitespace::StylisticNoEolWhitespace,
  ));
  registry.register(Box::new(
    stylistic_no_extra_semicolons::StylisticNoExtraSemicolons,
  ));
  registry.register(Box::new(
    stylistic_number_leading_zero::StylisticNumberLeadingZero,
  ));
  registry.register(Box::new(
        stylistic_declaration_block_semicolon_newline_after::StylisticDeclarationBlockSemicolonNewlineAfter,
    ));
  registry.register(Box::new(
    stylistic_declaration_colon_newline_after::StylisticDeclarationColonNewlineAfter,
  ));
  registry.register(Box::new(
    stylistic_selector_combinator_space_after::StylisticSelectorCombinatorSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_selector_attribute_operator_space_after::StylisticSelectorAttributeOperatorSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_block_opening_brace_space_before::StylisticBlockOpeningBraceSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_at_rule_name_case::StylisticAtRuleNameCase,
  ));
  registry.register(Box::new(
    stylistic_at_rule_semicolon_space_before::StylisticAtRuleSemicolonSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_block_opening_brace_newline_after::StylisticBlockOpeningBraceNewlineAfter,
  ));
  registry.register(Box::new(
    stylistic_media_feature_colon_space_after::StylisticMediaFeatureColonSpaceAfter,
  ));
  registry.register(Box::new(
        stylistic_selector_attribute_brackets_space_inside::StylisticSelectorAttributeBracketsSpaceInside,
    ));
  registry.register(Box::new(
    stylistic_selector_pseudo_class_case::StylisticSelectorPseudoClassCase,
  ));
  registry.register(Box::new(
    stylistic_value_list_comma_space_before::StylisticValueListCommaSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_value_list_max_empty_lines::StylisticValueListMaxEmptyLines,
  ));
  registry.register(Box::new(
        stylistic_selector_descendant_combinator_no_non_space::StylisticSelectorDescendantCombinatorNoNonSpace,
    ));
  registry.register(Box::new(
    stylistic_block_opening_brace_space_after::StylisticBlockOpeningBraceSpaceAfter,
  ));
  registry.register(Box::new(
    stylistic_block_closing_brace_space_before::StylisticBlockClosingBraceSpaceBefore,
  ));
  registry.register(Box::new(
    stylistic_no_empty_first_line::StylisticNoEmptyFirstLine,
  ));
  registry.register(Box::new(
    stylistic_media_feature_name_case::StylisticMediaFeatureNameCase,
  ));
  registry.register(Box::new(
    stylistic_function_comma_newline_after::StylisticFunctionCommaNewlineAfter,
  ));

  // SCSS-specific rules
  registry.register(Box::new(
    scss_at_else_closing_brace_newline_after::ScssAtElseClosingBraceNewlineAfter,
  ));
  registry.register(Box::new(
    scss_at_else_closing_brace_space_after::ScssAtElseClosingBraceSpaceAfter,
  ));
  registry.register(Box::new(
    scss_at_extend_no_missing_placeholder::ScssAtExtendNoMissingPlaceholder,
  ));
  registry.register(Box::new(scss_at_function_pattern::ScssAtFunctionPattern));
  registry.register(Box::new(
    scss_at_if_closing_brace_newline_after::ScssAtIfClosingBraceNewlineAfter,
  ));
  registry.register(Box::new(
    scss_at_if_closing_brace_space_after::ScssAtIfClosingBraceSpaceAfter,
  ));
  registry.register(Box::new(scss_at_if_no_null::ScssAtIfNoNull));
  registry.register(Box::new(
    scss_at_import_partial_extension::ScssAtImportPartialExtension,
  ));
  registry.register(Box::new(
    scss_at_import_partial_extension_disallowed_list::ScssAtImportPartialExtensionDisallowedList,
  ));
  registry.register(Box::new(
    scss_at_mixin_argumentless_call_parentheses::ScssAtMixinArgumentlessCallParentheses,
  ));
  registry.register(Box::new(
    scss_at_mixin_disallowed_list::ScssAtMixinDisallowedList,
  ));
  registry.register(Box::new(scss_at_mixin_pattern::ScssAtMixinPattern));
  registry.register(Box::new(scss_at_rule_no_unknown::ScssAtRuleNoUnknown));
  registry.register(Box::new(scss_comment_no_empty::ScssCommentNoEmpty));
  registry.register(Box::new(
    scss_declaration_nested_properties::ScssDeclarationNestedProperties,
  ));
  registry.register(Box::new(scss_declaration_nested_properties_no_divided_groups::ScssDeclarationNestedPropertiesNoDividedGroups));
  registry.register(Box::new(
    scss_dollar_variable_colon_space_after::ScssDollarVariableColonSpaceAfter,
  ));
  registry.register(Box::new(
    scss_dollar_variable_colon_space_before::ScssDollarVariableColonSpaceBefore,
  ));
  registry.register(Box::new(
    scss_dollar_variable_no_missing_interpolation::ScssDollarVariableNoMissingInterpolation,
  ));
  registry.register(Box::new(
    scss_dollar_variable_pattern::ScssDollarVariablePattern,
  ));
  registry.register(Box::new(
    scss_double_slash_comment_whitespace_inside::ScssDoubleSlashCommentWhitespaceInside,
  ));
  registry.register(Box::new(scss_function_no_unknown::ScssFunctionNoUnknown));
  registry.register(Box::new(
    scss_function_quote_no_quoted_strings_inside::ScssFunctionQuoteNoQuotedStringsInside,
  ));
  registry.register(Box::new(
    scss_function_unquote_no_unquoted_strings_inside::ScssFunctionUnquoteNoUnquotedStringsInside,
  ));
  registry.register(Box::new(
    scss_load_no_partial_leading_underscore::ScssLoadNoPartialLeadingUnderscore,
  ));
  registry.register(Box::new(
    scss_load_partial_extension::ScssLoadPartialExtension,
  ));
  registry.register(Box::new(
    scss_no_duplicate_dollar_variables::ScssNoDuplicateDollarVariables,
  ));
  registry.register(Box::new(scss_no_duplicate_mixins::ScssNoDuplicateMixins));
  registry.register(Box::new(
    scss_no_global_function_names::ScssNoGlobalFunctionNames,
  ));
  registry.register(Box::new(
    scss_operator_no_newline_after::ScssOperatorNoNewlineAfter,
  ));
  registry.register(Box::new(
    scss_operator_no_newline_before::ScssOperatorNoNewlineBefore,
  ));
  registry.register(Box::new(scss_operator_no_unspaced::ScssOperatorNoUnspaced));
  registry.register(Box::new(scss_partial_no_import::ScssPartialNoImport));
  registry.register(Box::new(
    scss_percent_placeholder_pattern::ScssPercentPlaceholderPattern,
  ));
  registry.register(Box::new(
    scss_selector_no_redundant_nesting_selector::ScssSelectorNoRedundantNestingSelector,
  ));
  registry.register(Box::new(
    scss_dollar_variable_empty_line_before::ScssDollarVariableEmptyLineBefore,
  ));
  registry.register(Box::new(scss_comment_no_loud::ScssCommentNoLoud));
  registry.register(Box::new(
    scss_at_mixin_parentheses_space_before::ScssAtMixinParenthesesSpaceBefore,
  ));
  registry.register(Box::new(
    scss_at_function_parentheses_space_before::ScssAtFunctionParenthesesSpaceBefore,
  ));
  registry.register(Box::new(
    scss_at_else_if_parentheses_space_before::ScssAtElseIfParenthesesSpaceBefore,
  ));
  registry.register(Box::new(
    scss_at_else_empty_line_before::ScssAtElseEmptyLineBefore,
  ));
  registry.register(Box::new(
    scss_double_slash_comment_inline::ScssDoubleSlashCommentInline,
  ));
  registry.register(Box::new(
    scss_double_slash_comment_empty_line_before::ScssDoubleSlashCommentEmptyLineBefore,
  ));
  registry.register(Box::new(
    scss_at_rule_conditional_no_parentheses::ScssAtRuleConditionalNoParentheses,
  ));
  registry.register(Box::new(
    scss_function_disallowed_list::ScssFunctionDisallowedList,
  ));
}
