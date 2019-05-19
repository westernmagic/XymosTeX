use crate::category::Category;
use crate::dimension::{Dimen, Unit};
use crate::glue::Glue;
use crate::list::VerticalListElem;
use crate::parser::Parser;
use crate::token::Token;

impl<'a> Parser<'a> {
    /// Handle generating an optionally indented horizontal mode box by
    /// entering horizontal mode and parsing the box there.
    fn handle_enter_horizontal_mode(
        &mut self,
        indent: bool,
    ) -> VerticalListElem {
        // TODO(xymostech): This will eventually potentially
        // produce a series of boxes instead of just one, if there
        // are line breaks. Handle that.
        let tex_box = self.parse_unrestricted_horizontal_box(indent);
        // TODO(xymostech): Add \parskip glue before the box.
        VerticalListElem::Box(tex_box)
    }

    /// Checks if a token is the start of something that only is valid in
    /// horizontal mode.
    fn is_horizontal_mode_head(&self, tok: &Token) -> bool {
        if let Token::Char(_, cat) = tok {
            if *cat == Category::Letter || *cat == Category::Other {
                return true;
            }
        }

        if self.state.is_token_equal_to_prim(tok, "hskip") {
            return true;
        }

        return false;
    }

    fn parse_vertical_list_elem(
        &mut self,
        group_level: &mut usize,
        internal: bool,
    ) -> Option<VerticalListElem> {
        let expanded_token = self.peek_expanded_token();
        let expanded_renamed_token = self.replace_renamed_token(expanded_token);
        match expanded_renamed_token {
            None => {
                if internal {
                    None
                } else {
                    panic!(r"Emergency stop, EOF found before \end");
                }
            }
            Some(ref tok) if self.is_horizontal_mode_head(tok) => {
                Some(self.handle_enter_horizontal_mode(true))
            }
            Some(Token::Char(_, cat)) => match cat {
                Category::Space => {
                    self.lex_expanded_token();
                    self.parse_vertical_list_elem(group_level, internal)
                }
                Category::BeginGroup => {
                    self.lex_expanded_token();
                    *group_level += 1;
                    self.state.push_state();
                    self.parse_vertical_list_elem(group_level, internal)
                }
                Category::EndGroup => {
                    if *group_level == 0 {
                        if internal {
                            None
                        } else {
                            panic!("Too many }'s!");
                        }
                    } else {
                        self.lex_expanded_token();
                        *group_level -= 1;
                        self.state.pop_state();
                        self.parse_vertical_list_elem(group_level, internal)
                    }
                }
                _ => panic!("unimplemented"),
            },
            Some(ref tok) if self.state.is_token_equal_to_prim(tok, "end") => {
                if internal {
                    panic!(r"You can't use \end in internal vertical mode")
                }
                self.lex_expanded_token();
                None
            }
            Some(ref tok) if self.state.is_token_equal_to_prim(tok, "par") => {
                // \par is completely ignored
                self.lex_expanded_token();
                self.parse_vertical_list_elem(group_level, internal)
            }
            Some(ref tok)
                if self.state.is_token_equal_to_prim(tok, "vskip") =>
            {
                self.lex_expanded_token();
                let glue = self.parse_glue();
                Some(VerticalListElem::VSkip(glue))
            }
            _ => {
                if self.is_assignment_head() {
                    self.parse_assignment();
                    self.parse_vertical_list_elem(group_level, internal)
                } else if self.is_next_expanded_token_in_set_of_primitives(&[
                    "indent", "noindent",
                ]) {
                    let tok = self.lex_expanded_token().unwrap();
                    let indent =
                        self.state.is_token_equal_to_prim(&tok, "indent");
                    Some(self.handle_enter_horizontal_mode(indent))
                } else if self.is_box_head() {
                    let maybe_tex_box = self.parse_box();
                    if let Some(tex_box) = maybe_tex_box {
                        // TODO(xymostech): Insert interline glue here.
                        Some(VerticalListElem::Box(tex_box))
                    } else {
                        self.parse_vertical_list_elem(group_level, internal)
                    }
                } else {
                    panic!("unimplemented");
                }
            }
        }
    }

    pub fn parse_vertical_list(
        &mut self,
        internal: bool,
    ) -> Vec<VerticalListElem> {
        let mut result = Vec::new();

        // The depth of the most recent box.
        // TODO(xymostech): Store this in the \prevdepth parameter
        let mut prev_depth = Dimen::from_unit(-1000.0, Unit::Point);

        // TODO(xymostech): Store these as \baselineskip, \lineskiplimit, and
        // \lineskip parameters
        let baselineskip =
            Glue::from_dimen(Dimen::from_unit(12.0, Unit::Point));
        let lineskiplimit = Dimen::from_unit(0.0, Unit::Point);
        let lineskip = Glue::from_dimen(Dimen::from_unit(1.0, Unit::Point));

        let mut group_level = 0;
        while let Some(elem) =
            self.parse_vertical_list_elem(&mut group_level, internal)
        {
            // Handle box elements specially so we can add interline glue
            if let VerticalListElem::Box(ref tex_box) = elem {
                // If prev_depth is -1000pt, don't add interline glue
                if prev_depth != Dimen::from_unit(-1000.0, Unit::Point) {
                    // Calculate how much interline glue we'd add if we just
                    // take into account baselineskip - prev_depth - box.height
                    let box_height = tex_box.height();
                    let total_skip = baselineskip.clone()
                        - Glue::from_dimen(*box_height + prev_depth);

                    // If the interline glue would be less than lineskiplimit,
                    // use lineskip instead.
                    let interline_glue = if total_skip.space < lineskiplimit {
                        lineskip.clone()
                    } else {
                        total_skip
                    };

                    result.push(VerticalListElem::VSkip(interline_glue));
                }

                // Keep track of the depth of the most recent box
                prev_depth = tex_box.depth().clone();
            }

            result.push(elem);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dimension::{FilDimen, FilKind, SpringDimen};
    use crate::testing::with_parser;

    fn assert_parses_to(lines: &[&str], expected_list: &[VerticalListElem]) {
        with_parser(lines, |parser| {
            assert_eq!(parser.parse_vertical_list(true), expected_list);
        });
    }

    #[test]
    fn it_parses_vertical_glue() {
        assert_parses_to(
            &[r"\vskip 1pt%", r"\vskip 0pt plus1fil%"],
            &[
                VerticalListElem::VSkip(Glue {
                    space: Dimen::from_unit(1.0, Unit::Point),
                    stretch: SpringDimen::Dimen(Dimen::zero()),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
                VerticalListElem::VSkip(Glue {
                    space: Dimen::zero(),
                    stretch: SpringDimen::FilDimen(FilDimen::new(
                        FilKind::Fil,
                        1.0,
                    )),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
            ],
        );
    }

    #[test]
    fn it_ignores_spaces() {
        assert_parses_to(
            &[r"\vskip 1pt     \vskip 1pt%"],
            &[
                VerticalListElem::VSkip(Glue {
                    space: Dimen::from_unit(1.0, Unit::Point),
                    stretch: SpringDimen::Dimen(Dimen::zero()),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
                VerticalListElem::VSkip(Glue {
                    space: Dimen::from_unit(1.0, Unit::Point),
                    stretch: SpringDimen::Dimen(Dimen::zero()),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
            ],
        );
    }

    #[test]
    fn it_parses_assignments() {
        assert_parses_to(
            &[r"\def\a{\vskip 1pt}", r"\a\a%"],
            &[
                VerticalListElem::VSkip(Glue {
                    space: Dimen::from_unit(1.0, Unit::Point),
                    stretch: SpringDimen::Dimen(Dimen::zero()),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
                VerticalListElem::VSkip(Glue {
                    space: Dimen::from_unit(1.0, Unit::Point),
                    stretch: SpringDimen::Dimen(Dimen::zero()),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
            ],
        );
    }

    #[test]
    fn it_handles_grouping() {
        assert_parses_to(
            &[
                r"\def\a{\vskip 1pt}",
                r"{",
                r"\def\a{\vskip 2pt}",
                r"\a",
                r"}",
                r"\a",
            ],
            &[
                VerticalListElem::VSkip(Glue {
                    space: Dimen::from_unit(2.0, Unit::Point),
                    stretch: SpringDimen::Dimen(Dimen::zero()),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
                VerticalListElem::VSkip(Glue {
                    space: Dimen::from_unit(1.0, Unit::Point),
                    stretch: SpringDimen::Dimen(Dimen::zero()),
                    shrink: SpringDimen::Dimen(Dimen::zero()),
                }),
            ],
        );
    }

    #[test]
    fn it_finishes_parsing_before_unmatched_close_group() {
        with_parser(&[r"{\vskip 1pt{{}\vskip 1pt}{}}}%"], |parser| {
            assert_eq!(
                parser.parse_vertical_list(true),
                &[
                    VerticalListElem::VSkip(Glue {
                        space: Dimen::from_unit(1.0, Unit::Point),
                        stretch: SpringDimen::Dimen(Dimen::zero()),
                        shrink: SpringDimen::Dimen(Dimen::zero()),
                    }),
                    VerticalListElem::VSkip(Glue {
                        space: Dimen::from_unit(1.0, Unit::Point),
                        stretch: SpringDimen::Dimen(Dimen::zero()),
                        shrink: SpringDimen::Dimen(Dimen::zero()),
                    }),
                ]
            );

            assert_eq!(
                parser.lex_unexpanded_token(),
                Some(Token::Char('}', Category::EndGroup))
            );
        });
    }

    #[test]
    #[should_panic(expected = r"You can't use \end in internal vertical mode")]
    fn it_should_fail_with_end_in_internal_vertical_mode() {
        assert_parses_to(&[r"\vskip 0pt\end%"], &[]);
    }

    #[test]
    fn it_ends_non_internal_vertical_mode() {
        with_parser(&[r"\vskip 0pt\vskip 1pt\end a%"], |parser| {
            assert_eq!(
                parser.parse_vertical_list(false),
                &[
                    VerticalListElem::VSkip(Glue {
                        space: Dimen::from_unit(0.0, Unit::Point),
                        stretch: SpringDimen::Dimen(Dimen::zero()),
                        shrink: SpringDimen::Dimen(Dimen::zero()),
                    }),
                    VerticalListElem::VSkip(Glue {
                        space: Dimen::from_unit(1.0, Unit::Point),
                        stretch: SpringDimen::Dimen(Dimen::zero()),
                        shrink: SpringDimen::Dimen(Dimen::zero()),
                    }),
                ]
            );

            assert_eq!(
                parser.lex_unexpanded_token(),
                Some(Token::Char('a', Category::Letter))
            );
        });
    }

    #[test]
    #[should_panic(expected = "Too many }'s")]
    fn it_should_fail_with_too_many_end_groups() {
        with_parser(&["{{}{{}}}}%"], |parser| {
            parser.parse_vertical_list(false);
        });
    }

    #[test]
    #[should_panic(expected = r"EOF found before \end")]
    fn it_should_fail_with_no_end() {
        with_parser(&[r"\vskip 0pt%"], |parser| {
            parser.parse_vertical_list(false);
        });
    }

    #[test]
    fn it_parses_box_elements() {
        with_parser(
            &[
                r"\setbox0=\hbox{a}%",
                r"\setbox1=\hbox{b}%",
                r"\setbox2=\hbox{b}%",
                r"\vskip 1pt%",
                r"\hbox{a}%",
                r"\vskip 2pt%",
                r"\box2",
            ],
            |parser| {
                parser.parse_assignment();
                parser.parse_assignment();

                let box0 = parser.state.get_box(0).unwrap();
                let box1 = parser.state.get_box(1).unwrap();

                let interline_glue = Dimen::from_unit(12.0, Unit::Point)
                    - *box0.depth()
                    - *box1.height();

                assert_eq!(
                    parser.parse_vertical_list(true),
                    &[
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(1.0, Unit::Point)
                        )),
                        VerticalListElem::Box(box0),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(2.0, Unit::Point)
                        )),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            interline_glue
                        )),
                        VerticalListElem::Box(box1),
                    ]
                );
            },
        );
    }

    #[test]
    fn it_parses_hboxes_after_noindent() {
        with_parser(
            &[
                r"\setbox0=\hbox{a}%",
                r"\setbox1=\hbox{g}%",
                r"\vskip 1pt%",
                r"\noindent a\par%",
                r"\vskip 2pt%",
                r"\noindent g\par%",
            ],
            |parser| {
                parser.parse_assignment();
                parser.parse_assignment();

                let box0 = parser.state.get_box(0).unwrap();
                let box1 = parser.state.get_box(1).unwrap();

                let interline_glue = Dimen::from_unit(12.0, Unit::Point)
                    - *box0.depth()
                    - *box1.height();

                assert_eq!(
                    parser.parse_vertical_list(true),
                    &[
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(1.0, Unit::Point)
                        )),
                        VerticalListElem::Box(box0),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(2.0, Unit::Point)
                        )),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            interline_glue
                        )),
                        VerticalListElem::Box(box1),
                    ]
                );
            },
        );
    }

    #[test]
    fn it_parses_hboxes_after_indent() {
        with_parser(
            &[
                r"\setbox2=\hbox{}%",
                r"\wd2=20pt%",
                r"\setbox0=\hbox{\copy2 a}%",
                r"\setbox1=\hbox{\copy2 g}%",
                r"\vskip 1pt%",
                r"\indent a\par%",
                r"\vskip 2pt%",
                r"\indent g\par%",
            ],
            |parser| {
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();

                let box0 = parser.state.get_box(0).unwrap();
                let box1 = parser.state.get_box(1).unwrap();

                let interline_glue = Dimen::from_unit(12.0, Unit::Point)
                    - *box0.depth()
                    - *box1.height();

                assert_eq!(
                    parser.parse_vertical_list(true),
                    &[
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(1.0, Unit::Point)
                        )),
                        VerticalListElem::Box(box0),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(2.0, Unit::Point)
                        )),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            interline_glue
                        )),
                        VerticalListElem::Box(box1),
                    ]
                );
            },
        );
    }

    #[test]
    fn it_enters_horizontal_mode_after_horizontal_material() {
        with_parser(
            &[
                r"\setbox0=\hbox{}%",
                r"\wd0=20pt%",
                r"\setbox1=\hbox{\copy0 a}%",
                r"\setbox2=\hbox{\copy0 @}%",
                r"\setbox3=\hbox{\copy0 \hskip1pt}%",
                r"a\par%",
                r"@\par%",
                r"\hskip 1pt\par%",
            ],
            |parser| {
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();

                let box1 = parser.state.get_box(1).unwrap();
                let box2 = parser.state.get_box(2).unwrap();
                let box3 = parser.state.get_box(3).unwrap();

                let interline_glue1 = Dimen::from_unit(12.0, Unit::Point)
                    - *box1.depth()
                    - *box2.height();
                let interline_glue2 = Dimen::from_unit(12.0, Unit::Point)
                    - *box2.depth()
                    - *box3.height();

                assert_eq!(
                    parser.parse_vertical_list(true),
                    &[
                        VerticalListElem::Box(box1),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            interline_glue1
                        )),
                        VerticalListElem::Box(box2),
                        VerticalListElem::VSkip(Glue::from_dimen(
                            interline_glue2
                        )),
                        VerticalListElem::Box(box3),
                    ]
                );
            },
        );
    }

    #[test]
    fn it_adds_interline_glue() {
        with_parser(
            &[
                r"\setbox0=\hbox{}%",
                r"\dp0=5pt%",
                r"\setbox1=\hbox{}%",
                r"\ht1=5pt%",
                r"\dp1=8pt%",
                r"\setbox2=\hbox{}%",
                r"\ht2=5pt%",
                r"\copy0%",
                r"\copy1%",
                r"\copy2%",
            ],
            |parser| {
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();
                parser.parse_assignment();

                assert_eq!(
                    parser.parse_vertical_list(true),
                    &[
                        VerticalListElem::Box(parser.state.get_box(0).unwrap()),
                        // 12pt - 5pt - 5pt = 2pt of interline glue
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(2.0, Unit::Point)
                        )),
                        VerticalListElem::Box(parser.state.get_box(1).unwrap()),
                        // 12pt - 8pt - 5pt = -1pt
                        // -1pt < 0pt (lineskiplimit), so we end up with
                        // lineskip (1pt) interline glue
                        VerticalListElem::VSkip(Glue::from_dimen(
                            Dimen::from_unit(1.0, Unit::Point)
                        )),
                        VerticalListElem::Box(parser.state.get_box(2).unwrap()),
                    ]
                );
            },
        );
    }

    #[test]
    fn it_ignores_par() {
        with_parser(&[r"\vskip1pt", r"", r"\vskip1pt%"], |parser| {
            assert_eq!(
                parser.parse_vertical_list(true),
                &[
                    VerticalListElem::VSkip(Glue::from_dimen(
                        Dimen::from_unit(1.0, Unit::Point)
                    )),
                    VerticalListElem::VSkip(Glue::from_dimen(
                        Dimen::from_unit(1.0, Unit::Point)
                    )),
                ]
            );
        });
    }
}
