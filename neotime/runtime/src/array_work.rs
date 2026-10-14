//! The array questions and work the interpreter does natively, so apps carry no loops for them.

use super::{Failure, Interpreter, SystemValues, Value};
use crate::instruction::{ArrayQuestion, ArrayWork};
use crate::state::AppState;

// ============================================================================
// Grids
// ============================================================================

/// The longest side of a grid that `LifeStep` and the slides take, which sets the size of their row buffers.
pub const MOST_GRID_SIDE: usize = 16;

/// The longest array `Inversions` counts in, since it compares every pair.
pub const MOST_INVERSION_ELEMENTS: usize = 64;

/// How many bytes of a file `FileBytes` reads at a time, so a long array never needs a long buffer.
const FILE_CHUNK_BYTES: usize = 32;

/// Directions as the swipe event gives them.
const UP: Value = 0;
const DOWN: Value = 1;
const LEFT: Value = 2;
const RIGHT: Value = 3;

/// `SlideChanges` with this direction asks about all four.
const ANY_DIRECTION: Value = 4;

/// An array seen as rows of `width` cells, numbered across each row from the top left.
#[derive(Clone, Copy)]
struct Grid {
    width: usize,
    rows: usize,
}

impl Grid {
    fn new(length: usize, width: Value) -> Result<Grid, Failure> {
        let width = usize::try_from(width).map_err(|_| Failure::IndexOutOfRange)?;
        if width == 0 || !length.is_multiple_of(width) {
            return Err(Failure::IndexOutOfRange);
        }
        Ok(Grid {
            width,
            rows: length / width,
        })
    }

    /// The lines a slide moves tiles along, and how long each is.
    fn slide_lines(self, direction: Value) -> Result<(usize, usize), Failure> {
        let (count, length) = match direction {
            UP | DOWN => (self.width, self.rows),
            LEFT | RIGHT => (self.rows, self.width),
            _ => return Err(Failure::IndexOutOfRange),
        };
        match length <= MOST_GRID_SIDE {
            true => Ok((count, length)),
            false => Err(Failure::IndexOutOfRange),
        }
    }

    /// The cell at `place` along slide line `line`, counted from the edge the tiles move to.
    fn slide_cell(self, direction: Value, line: usize, place: usize) -> usize {
        match direction {
            UP => place * self.width + line,
            DOWN => (self.rows - 1 - place) * self.width + line,
            LEFT => line * self.width + place,
            _ => line * self.width + self.width - 1 - place,
        }
    }
}

/// Slides one line of tile levels towards its start, merging each pair of equal tiles once into one a level up.
/// Gives the points scored, two to the power of each merged tile's new level, and whether the line changed.
fn slide_line(line: &[Value], slid: &mut [Value]) -> (Value, bool) {
    let mut placed = 0;
    let mut waiting = 0;
    let mut points = 0;
    for &level in line.iter().filter(|&&level| level != 0) {
        if waiting == level {
            slid[placed] = level + 1;
            points += 1i64 << (level + 1).clamp(0, 62);
            placed += 1;
            waiting = 0;
        } else {
            if waiting != 0 {
                slid[placed] = waiting;
                placed += 1;
            }
            waiting = level;
        }
    }
    if waiting != 0 {
        slid[placed] = waiting;
        placed += 1;
    }
    slid[placed..line.len()].fill(0);
    (points, slid[..line.len()] != *line)
}

// ============================================================================
// Running them
// ============================================================================

impl Interpreter {
    fn array_length(state: &AppState, array: u8) -> Result<usize, Failure> {
        Ok(Self::array_placement(state, array)?.length as usize)
    }

    fn element_of(&self, state: &AppState, array: u8, index: usize) -> Result<Value, Failure> {
        self.read_element(state, array, index as Value, true)
    }

    /// Answers `question` about `array`, taking its arguments from the stack and pushing the answer.
    #[inline(never)]
    pub(super) fn ask_array(
        &mut self,
        state: &AppState,
        question: ArrayQuestion,
        array: u8,
    ) -> Result<(), Failure> {
        let length = Self::array_length(state, array)?;
        let answer = match question {
            ArrayQuestion::CountOf => {
                let value = self.pop()?;
                let mut count = 0;
                for index in 0..length {
                    count += (self.element_of(state, array, index)? == value) as Value;
                }
                count
            }
            ArrayQuestion::NthIndexOf => {
                let wanted = self.pop()?;
                let value = self.pop()?;
                let mut seen = 0;
                let mut found = -1;
                for index in 0..length {
                    if found < 0 && self.element_of(state, array, index)? == value {
                        if seen == wanted {
                            found = index as Value;
                        }
                        seen += 1;
                    }
                }
                found
            }
            ArrayQuestion::LineThrough => {
                let value = self.pop()?;
                let cell = self.pop()?;
                let grid = Grid::new(length, self.pop()?)?;
                self.line_through(state, array, grid, cell, value)?
            }
            ArrayQuestion::Inversions => {
                if length > MOST_INVERSION_ELEMENTS {
                    return Err(Failure::IndexOutOfRange);
                }
                let mut inversions = 0;
                for earlier in 0..length {
                    let value = self.element_of(state, array, earlier)?;
                    for later in earlier + 1..length {
                        inversions += (self.element_of(state, array, later)? < value) as Value;
                    }
                }
                inversions
            }
            ArrayQuestion::SlideChanges | ArrayQuestion::SlidePoints => {
                let direction = self.pop()?;
                let grid = Grid::new(length, self.pop()?)?;
                let directions = match direction {
                    ANY_DIRECTION if question == ArrayQuestion::SlideChanges => UP..ANY_DIRECTION,
                    _ => direction..direction + 1,
                };
                let mut changes = false;
                let mut points = 0;
                for direction in directions {
                    let (line_points, line_changes) = self.slide(state, array, None, grid, direction)?;
                    points += line_points;
                    changes |= line_changes;
                }
                match question {
                    ArrayQuestion::SlideChanges => changes as Value,
                    _ => points,
                }
            }
        };
        self.push(answer)
    }

    /// Does `work` on `source`, writing `destination` or, when comparing them, pushing whether they are equal.
    #[inline(never)]
    pub(super) fn work_array(
        &mut self,
        state: &AppState,
        work: ArrayWork,
        destination: u8,
        source: u8,
        random_number: &dyn Fn() -> u32,
    ) -> Result<(), Failure> {
        let length = Self::array_length(state, source)?;
        if Self::array_length(state, destination)? != length {
            return Err(Failure::IndexOutOfRange);
        }
        match work {
            ArrayWork::Equal => {
                let mut equal = true;
                for index in 0..length {
                    equal &= self.element_of(state, source, index)?
                        == self.element_of(state, destination, index)?;
                }
                self.push(equal as Value)
            }
            ArrayWork::LifeStep => {
                let live_value = self.pop()?;
                let grid = Grid::new(length, self.pop()?)?;
                self.life_step(state, destination, source, grid, live_value)
            }
            ArrayWork::Slide => {
                let direction = self.pop()?;
                let grid = Grid::new(length, self.pop()?)?;
                self.slide(state, source, Some(destination), grid, direction)
                    .map(|_| ())
            }
            // The interpreter reads files itself, since only it has the system and the path's text.
            ArrayWork::FileBytes => Err(Failure::BadInstruction),
            ArrayWork::Shuffle => {
                let count = self.pop()?;
                let first = self.pop()?;
                let (Ok(first), Ok(count)) = (usize::try_from(first), usize::try_from(count)) else {
                    return Err(Failure::IndexOutOfRange);
                };
                if first + count > length {
                    return Err(Failure::IndexOutOfRange);
                }
                if destination != source {
                    for index in 0..length {
                        let value = self.element_of(state, source, index)?;
                        self.write_element(state, destination, index as Value, value, true)?;
                    }
                }
                // Fisher and Yates: each place from the end takes a random one of the places not yet settled.
                for place in (1..count).rev() {
                    let other = random_number() as usize % (place + 1);
                    let (here, there) = (first + place, first + other);
                    let held = self.element_of(state, destination, here)?;
                    let swapped = self.element_of(state, destination, there)?;
                    self.write_element(state, destination, here as Value, swapped, true)?;
                    self.write_element(state, destination, there as Value, held, true)?;
                }
                Ok(())
            }
            ArrayWork::PushLast | ArrayWork::PushFirst => {
                let value = self.pop()?;
                if length == 0 {
                    return Ok(());
                }
                // Each place is read before anything is written over it, so the source may be the destination.
                let order: &mut dyn Iterator<Item = usize> = match work {
                    ArrayWork::PushLast => &mut (0..length - 1),
                    _ => &mut (1..length).rev(),
                };
                for index in order {
                    let from = if work == ArrayWork::PushLast {
                        index + 1
                    } else {
                        index - 1
                    };
                    let moved = self.element_of(state, source, from)?;
                    self.write_element(state, destination, index as Value, moved, true)?;
                }
                let end = if work == ArrayWork::PushLast {
                    length - 1
                } else {
                    0
                };
                self.write_element(state, destination, end as Value, value, true)
            }
        }
    }

    /// Writes the first bytes of the file at `path` into `array`, one per element, with 0 past the file's end.
    #[inline(never)]
    pub(super) fn read_file_bytes(
        &mut self,
        state: &AppState,
        array: u8,
        path: &str,
        system: &impl SystemValues,
    ) -> Result<(), Failure> {
        let length = Self::array_length(state, array)?;
        let mut chunk = [0u8; FILE_CHUNK_BYTES];
        let mut offset = 0;
        let mut file_ended = false;

        while offset < length {
            let wanted = (length - offset).min(FILE_CHUNK_BYTES);
            let read = match file_ended {
                true => 0,
                false => system
                    .read_file(path, offset as u32, &mut chunk[..wanted])
                    .unwrap_or(0)
                    .min(wanted),
            };
            file_ended = read < wanted;
            chunk[read..wanted].fill(0);
            for (place, byte) in chunk[..wanted].iter().enumerate() {
                self.write_element(state, array, (offset + place) as Value, *byte as Value, true)?;
            }
            offset += wanted;
        }
        Ok(())
    }

    /// The longest line of `value` through `cell`, counting the cell itself as holding it.
    fn line_through(
        &self,
        state: &AppState,
        array: u8,
        grid: Grid,
        cell: Value,
        value: Value,
    ) -> Result<Value, Failure> {
        let cell = usize::try_from(cell).map_err(|_| Failure::IndexOutOfRange)?;
        if cell >= grid.width * grid.rows {
            return Err(Failure::IndexOutOfRange);
        }
        let (row, column) = ((cell / grid.width) as isize, (cell % grid.width) as isize);
        let mut longest = 0;
        for (row_step, column_step) in [(0, 1), (1, 0), (1, 1), (1, -1)] {
            let mut length = 1;
            for sign in [1, -1] {
                let mut distance = 1;
                loop {
                    let next_row = row + sign * row_step * distance;
                    let next_column = column + sign * column_step * distance;
                    let inside = (0..grid.rows as isize).contains(&next_row)
                        && (0..grid.width as isize).contains(&next_column);
                    if !inside {
                        break;
                    }
                    let next = next_row as usize * grid.width + next_column as usize;
                    if self.element_of(state, array, next)? != value {
                        break;
                    }
                    length += 1;
                    distance += 1;
                }
            }
            longest = longest.max(length);
        }
        Ok(longest)
    }

    /// Slides every line of `source` one way, writing the result to `destination` if there is one.
    /// Gives the points the merges score and whether anything moved.
    fn slide(
        &mut self,
        state: &AppState,
        source: u8,
        destination: Option<u8>,
        grid: Grid,
        direction: Value,
    ) -> Result<(Value, bool), Failure> {
        let (line_count, line_length) = grid.slide_lines(direction)?;
        let mut line = [0; MOST_GRID_SIDE];
        let mut slid = [0; MOST_GRID_SIDE];
        let mut points = 0;
        let mut changes = false;
        for line_number in 0..line_count {
            for (place, level) in line[..line_length].iter_mut().enumerate() {
                *level = self.element_of(state, source, grid.slide_cell(direction, line_number, place))?;
            }
            let (line_points, line_changes) = slide_line(&line[..line_length], &mut slid);
            points += line_points;
            changes |= line_changes;
            if let Some(destination) = destination {
                for (place, &level) in slid[..line_length].iter().enumerate() {
                    let cell = grid.slide_cell(direction, line_number, place);
                    self.write_element(state, destination, cell as Value, level, true)?;
                }
            }
        }
        Ok((points, changes))
    }

    /// Writes the next generation of `source` to `destination`, which may be the same array: each row is
    /// written only after the rows around it were read as they were.
    fn life_step(
        &mut self,
        state: &AppState,
        destination: u8,
        source: u8,
        grid: Grid,
        live_value: Value,
    ) -> Result<(), Failure> {
        let Grid { width, rows } = grid;
        if width > MOST_GRID_SIDE {
            return Err(Failure::IndexOutOfRange);
        }
        if rows == 0 {
            return Ok(());
        }
        let read_row = |interpreter: &Self, row: usize| -> Result<[bool; MOST_GRID_SIDE], Failure> {
            let mut alive = [false; MOST_GRID_SIDE];
            for (column, cell) in alive[..width].iter_mut().enumerate() {
                *cell = interpreter.element_of(state, source, row * width + column)? != 0;
            }
            Ok(alive)
        };
        let first = read_row(self, 0)?;
        let mut above = read_row(self, rows - 1)?;
        let mut current = first;
        for row in 0..rows {
            let below = match row + 1 == rows {
                true => first,
                false => read_row(self, row + 1)?,
            };
            for column in 0..width {
                let left = (column + width - 1) % width;
                let right = (column + 1) % width;
                let neighbours = [above, current, below]
                    .iter()
                    .flat_map(|line| [line[left], line[column], line[right]])
                    .filter(|&alive| alive)
                    .count()
                    - current[column] as usize;
                let lives = neighbours == 3 || (current[column] && neighbours == 2);
                let value = if lives { live_value } else { 0 };
                self.write_element(state, destination, (row * width + column) as Value, value, true)?;
            }
            above = current;
            current = below;
        }
        Ok(())
    }
}
