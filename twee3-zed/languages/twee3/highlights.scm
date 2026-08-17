(passage_name) @function
(macro_name) @keyword
(link_text) @string.special
(tags) @property
(metadata) @string.special
(tag) @string
(string) @string
(variable) @variable
(number) @number
(boolean) @boolean
(keyword_operator) @keyword
(operator) @operator
(image_source) @string.link
(comment) @comment
(html_comment) @comment

; Text Formatting
((text_formatting) @markup.bold (#match? @markup.bold "^''"))
((text_formatting) @markup.italic (#match? @markup.italic "^//"))
((text_formatting) @markup.underline (#match? @markup.underline "^__"))
((text_formatting) @markup.strikethrough (#match? @markup.strikethrough "^=="))

; Punctuation
"::" @punctuation.special
"[[" @punctuation.bracket
"]]" @punctuation.bracket
"<<" @punctuation.bracket
">>" @punctuation.bracket
"<</" @punctuation.bracket
"][" @punctuation.bracket
"|" @punctuation.delimiter
"->" @punctuation.delimiter
"<-" @punctuation.delimiter
"[img[" @punctuation.bracket
