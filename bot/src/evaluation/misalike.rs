use arrayvec::ArrayVec;
use libtetris::*;
use super::*;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MisalikeEvaluator {
    pub in_row_transitions: i32,

    pub t_piece_in_hold: i32,
    pub i_piece_in_hold: i32,

    pub open_hole: i32,
    pub closed_hole: i32,
}

impl Evaluator for MisalikeEvaluator {
    fn info(&self) -> Info {
        vec![
            ("Misalike".to_string(), None),
            ("ai style".to_string(), None)
        ]
    }

    fn evaluate(&mut self, lock: &LockResult, board: &Board, soft_dropped: bool) -> Evaluation {
        // Context: We're trying to translate this function from MisaMino:
        // https://github.com/misakamm/MisaMino/blob/master/tetris_ai/tetris_ai.cpp#L45
        // Note: the board is y-down; high y = low on the board, low y = high on the board
        // I *think* y=0 is row 20 and y=19 is row 1. It's hard to tell. I don't know what value
        // pool_h takes on. I really hope it's 20.

        let mut score = 0;

        // Lines 73 to 89
        // This finds the highest point on the board (beg_y), the column heights (min_y), the x
        // value of the lowest column (maxy_index), and the number of columns with the same height
        // as the lowest column minus 1 (maxy_cnt). I'm pretty sure miny_val ends up being equal to
        // beg_y.
        let mut highest_y = 0;
        let mut lowest_column = 0;
        let mut extra_lowest_columns = 0;
        for (x, &height) in board.column_heights().iter().enumerate() {
            highest_y = highest_y.max(height);
            if height < board.column_heights()[lowest_column] {
                lowest_column = x;
                extra_lowest_columns = 0;
            } else if height == board.column_heights()[lowest_column] {
                extra_lowest_columns += 1;
            }
        }

        // Lines 90 to 109
        // This finds the number of transitions between empty and solid cells exist when you move
        // along the rows (transitions), starting at the conceptually solid left border all the way
        // to the conceptually solid right border. Interestingly, empty rows don't increment the
        // number of transitions at all, acting as if they were solid. This also collects into an
        // array the number of empty cells in each row (empty). The loop starts at the topmost row,
        // so empty rows are in practice never encountered and never have their entry in the empty
        // array set, leaving them at 0. Finally, score is incremented according to the ai_param.
        let mut row_empty_count = [0; 40];
        let mut in_row_transitions = 0;
        for y in 0..highest_y {
            let mut last = true;
            assert!(!board.get_row(y).is_empty());
            for x in 0..10 {
                if board.occupied(x, y) {
                    if !last {
                        in_row_transitions += 1;
                        last = true;
                    }
                } else {
                    row_empty_count[y as usize] += 1;
                    if last {
                        in_row_transitions += 1;
                        last = false;
                    }
                }
            }
            if !last {
                in_row_transitions += 1;
            }
        }
        score += self.in_row_transitions * in_row_transitions / 10;

        // Line 111
        // This sets the height of column 11 (one off the right of the screen) to the height
        // of column 9. This is kinda important-ish for hole detection later. We use a different
        // strategy there.

        // Line 114 to 119
        // This checks if a T piece or an I piece is in hold and changes score appropriately.
        score -= match board.hold_piece() {
            Some(Piece::T) => self.t_piece_in_hold,
            Some(Piece::I) => self.i_piece_in_hold,
            _ => 0
        };

        // Line 120 to 133
        // This finds the longest length run of flat ground at the lowest point on the stack
        // (maxy_flat_cnt) and changes maxy_index to be the x value of the start of that longest
        // run. It actually checks runs starting in the middle of runs its already checked, which
        // is kinda odd but whatever.
        // The code is kinda hard to decipher, so here's some descriptions of what the code does:
        // ybeg is the y value of the first solid cell of the lowest column.
        // rowdata is the row above ybeg. empty has 1s where empty cells in the row are.
        // The loop uses the fact that lowest_column is the lowest x-valued lowest column.
        // Columns that are not the lowest are skipped.
        // b and b1 are just the x values currently being checked; b is the start of the run and b1
        // is the current position in the run being checked.
        let mut lowest_point_run_length = 0;
        if extra_lowest_columns != 0 {
            let row = board.get_row(board.column_heights()[lowest_column]);
            let mut start_x = lowest_column;
            while start_x < 10 {
                let mut run_length = 1;
                while start_x + run_length < 10 && !row.get(start_x + run_length) {
                    run_length += 1;
                }
                if run_length > lowest_point_run_length {
                    lowest_point_run_length = run_length;
                    lowest_column = start_x;
                }
                start_x += run_length;
            }
        }

        // Line 229 to 235
        // Counts the number of empty cells underneath solid cells (pool_total_cell).
        let mut empty_cells_below_stack = 0;
        for x in 0..10 {
            for y in 0..board.column_heights()[x] {
                if !board.occupied(x as i32, y) {
                    empty_cells_below_stack += 1;
                }
            }
        }

        // Line 236 to 288
        // This loop determines some information about the holes on the board and calculates the
        // hole score. A hole is defined as any empty cell that is either: a) below the topmost
        // solid cell, or b) 6 cells below the topmost solid cell of the lower of the two adjacent
        // columns. If soft dropping is allowed, a hole is "open" if (basically) it can be filled by
        // a possibly-floating L or J tuck. Otherwise, it is "closed".
        // In many cases, holes aren't counted if they're above the skyline. I will omit this here.

        // Operates on columns
        // first_hole_y[column] is set to below bottom of the board
        // loops starts at either the top of the current column or 6 below the
        // top of the shorter of the adjacent columns
        // The code acceses min_y[-1]. This is undefined behaviour. I'll just remove that bit.
        // Anyways, last is true if the above cell is empty. Nothing interesting happens
        // if the current cell is filled.
        // factor is 1.0 when y below the bottom of the board (?), and ranges from 1.0 to 3.0 as
        // you go from the bottom of the playfield to the skyline
        // column holes is incremented
        // softdropEnable() is always true for our purposes
        // (now, find "open holes")
        // if this isn't one of the leftmost two columns:
        //     if both the two leftmost columns are shorted than current y, then
        //     hole_score is increased
        //     if we're below the skyline, x_op_holes[y] is incremented
        //     go to next loop iteration
        // if this isn't onw of the rightmost two columns:
        //     same thing as above, but on the other side
        // So now we know that this isn't an "open hole".
        // if below the skyline, increment the number of holes in that column
        // if it's the highest hole in the column, record it
        // if the above cell is empty, the score is hole/2. Otherwise it's hole*2.
        // if the above cell is empty and we're below the skyline, renholes for that row is inc'd
        // hole scrore is increased by the above amount * factor
        let mut column_holes = [0; 10];
        let mut row_open_holes = [0; 40];
        let mut column_first_closed_hole = [-1; 10];
        let mut row_ren_closed_holes = [0; 40];
        let mut hole_score = 0.0;
        let mut hole_count = 0;

        for x in 0..10 {
            let hole_candidate_height = (board.column_heights()[x] - 1).max(
                match x {
                    0 => board.column_heights()[x+1],
                    9 => board.column_heights()[x-1],
                    _ => board.column_heights()[x-1].min(board.column_heights()[x+1])
                } - 6
            );
            let mut above_cell_empty = false;
            for y in (0..hole_candidate_height).rev() {
                if !board.occupied(x as i32, y) {
                    let factor = y as f64 / 10.0 + 1.0;
                    column_holes[x] += 1;

                    // if softdrop (might implement hard drop-only later)
                    if x > 1 {
                        if board.column_heights()[x-1] <= y && board.column_heights()[x-2] <= y {
                            // open hole
                            hole_score += self.open_hole as f64 * factor;
                            row_open_holes[y as usize] += 1;
                            above_cell_empty = true;
                            continue
                        }
                    }
                    if x < 8 {
                        if board.column_heights()[x+1] <= y && board.column_heights()[x+2] <= y {
                            // open hole
                            hole_score += self.open_hole as f64 * factor;
                            row_open_holes[y as usize] += 1;
                            above_cell_empty = true;
                            continue
                        }
                    }
                    // closed hole

                    if column_first_closed_hole[x] == -1 {
                        column_first_closed_hole[x] = y;
                    }

                    if above_cell_empty {
                        hole_score += (self.closed_hole / 2) as f64 * factor;
                        row_ren_closed_holes[y as usize] += 1;
                    } else {
                        hole_score += (self.closed_hole * 2) as f64 * factor;
                    }

                    hole_count += 1;
                    above_cell_empty = true;
                } else {
                    above_cell_empty = false;
                }
            }
        }

        unimplemented!()
    }
}