(passage
  (passage_header
    (tags
      (tag_list
        (tag) @_tag (#eq? @_tag "script"))))
  (passage_content) @injection.content
  (#set! injection.language "javascript"))

(passage
  (passage_header
    (tags
      (tag_list
        (tag) @_tag (#eq? @_tag "stylesheet"))))
  (passage_content) @injection.content
  (#set! injection.language "css"))
