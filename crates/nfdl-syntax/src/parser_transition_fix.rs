    fn parse_transition(&mut self, from_state: &str) -> Result<Transition, ParseError> {
        // assume we are at "on"
        if let Token::Ident(s) = &self.current {
            if s == "on" {
                self.advance();
            }
        }

        let msg_type = if let Token::Ident(m) = &self.current {
            let m = m.clone();
            self.advance();
            m
        } else { "Msg".to_string() };

        let mut guard = None;
        if let Token::Ident(g) = &self.current {
            if g == "guard" {
                self.advance();
                if self.current == Token::LParen { self.advance(); }
                guard = Some(self.parse_expr().unwrap_or(Expr::Int(1)));
                while self.current != Token::RParen && self.current != Token::Eof {
                    self.advance();
                }
                if self.current == Token::RParen { self.advance(); }
            }
        }

        // robustly find the next Ident as to_state (skip symbols like -> - > etc)
        while !matches!(&self.current, Token::Ident(_)) && self.current != Token::Eof {
            self.advance();
        }

        let to_state = if let Token::Ident(t) = &self.current {
            let t = t.clone();
            self.advance();
            t
        } else { "NEXT".to_string() };

        // skip action block { ... }
        if self.current == Token::LBrace {
            let mut depth = 0;
            while self.current != Token::Eof {
                if self.current == Token::LBrace { depth += 1; }
                if self.current == Token::RBrace { depth -= 1; if depth == 0 { self.advance(); break; } }
                self.advance();
            }
        }
        if self.current == Token::Semicolon { self.advance(); }

        Ok(Transition {
            from_state: Some(from_state.to_string()),
            msg_type,
            guard,
            to_state,
            actions: vec![],
        })
    }
