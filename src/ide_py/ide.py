import sys
import pygame
from pygame.locals import *

# ----------------------------------------------------------------------
# 配色方案
# ----------------------------------------------------------------------
COLORS = {
    'background': (20, 20, 30),
    'grid_line': (40, 40, 55),
    'space_dot': (80, 80, 100, 40),
    'leading_space_bg': (60, 80, 120, 25),
    'symbol_plus': (126, 182, 255),
    'symbol_star': (179, 157, 219),
    'symbol_backtick': (255, 171, 145),
    'symbol_quote': (255, 171, 145),
    'symbol_colon': (129, 199, 132),
    'symbol_semicolon': (129, 199, 132),
    'symbol_dot': (255, 213, 79),
    'symbol_comma': (255, 213, 79),
    'invalid': (239, 154, 154),
    'status_bar_bg': (30, 30, 40),
    'status_bar_text': (200, 200, 220),
    'highlight': (255, 255, 180, 50)
}

SYMBOL_GROUPS = {
    '+': 'plus',
    '*': 'star',
    '`': 'backtick',
    "'": 'quote',
    ':': 'colon',
    ';': 'semicolon',
    '.': 'dot',
    ',': 'comma',
}

# ----------------------------------------------------------------------
# 解析源码
# ----------------------------------------------------------------------
def parse_source(source_lines):
    grid = []
    max_cols = 0
    for raw_line in source_lines:
        line = raw_line.rstrip('\n')
        row = []
        for col, ch in enumerate(line):
            if ch == ' ':
                cell_type = 'space'
            elif ch in SYMBOL_GROUPS:
                cell_type = SYMBOL_GROUPS[ch]
            else:
                cell_type = 'invalid'
            row.append({'char': ch, 'type': cell_type, 'col': col})
        max_cols = max(max_cols, len(row))
        grid.append(row)
    return grid, max_cols, len(grid)

# ----------------------------------------------------------------------
# 计算前导空格数（当前符号之前连续空格的数量）
# ----------------------------------------------------------------------
def count_leading_spaces(row, col):
    """返回第 col 列（符号位置）之前连续空格的数量。如果 col 位置是空格，返回 0。"""
    if col >= len(row) or row[col]['char'] == ' ':
        return 0
    count = 0
    i = col - 1
    while i >= 0 and row[i]['char'] == ' ':
        count += 1
        i -= 1
    return count

# ----------------------------------------------------------------------
# 指令提示
# ----------------------------------------------------------------------
def get_cell_hint(cell, leading_spaces):
    ch = cell['char']
    t = cell['type']
    col = cell['col']
    if t == 'space':
        # 判断是否属于前导空格（后面紧跟着符号）
        return "空格 (前导)" if leading_spaces > 0 else "空格"
    elif t == 'plus':
        if col >= 5:
            op = f"Push {col - 5}"
        elif col == 1:
            op = "Dup"
        elif col == 2:
            op = "Swap"
        elif col == 3:
            op = "Rotate"
        elif col == 4:
            op = "Pop"
        else:
            op = f"+ (空格数 {col}，无效)"
        return f"{op} | 前导空格: {leading_spaces}"
    elif t == 'star':
        if leading_spaces == 0:
            op = "Add"
        elif leading_spaces == 1:
            op = "Sub"
        elif leading_spaces == 2:
            op = "Mul"
        elif leading_spaces == 3:
            op = "Div"
        elif leading_spaces == 4:
            op = "Mod"
        elif leading_spaces == 5:
            op = "Reverse"
        else:
            op = f"* (空格数 {leading_spaces}，无效)"
        return f"{op} | 前导空格: {leading_spaces}"
    elif t in ('backtick', 'quote'):
        if ch == '`':
            op = f"Mark {leading_spaces}"
        else:
            op = f"Jump {leading_spaces}"
        return f"{op} | 前导空格: {leading_spaces}"
    elif t in ('colon', 'semicolon'):
        if ch == ':':
            op = f"函数声明/调用分隔 (空格数 {leading_spaces})"
        else:
            op = "函数调用结束"
        return f"{op} | 前导空格: {leading_spaces}"
    elif t == 'dot':
        if leading_spaces == 0:
            op = "NumOut"
        elif leading_spaces == 1:
            op = "NumIn"
        else:
            op = f". (空格数 {leading_spaces}，无效)"
        return f"{op} | 前导空格: {leading_spaces}"
    elif t == 'comma':
        if leading_spaces == 0:
            op = "CharOut"
        elif leading_spaces == 1:
            op = "CharIn"
        else:
            op = f", (空格数 {leading_spaces}，无效)"
        return f"{op} | 前导空格: {leading_spaces}"
    else:
        return f"无效字符 | 前导空格: {leading_spaces}"

# ----------------------------------------------------------------------
# 主程序
# ----------------------------------------------------------------------
class StardustViewer:
    def __init__(self, source_lines=None):
        pygame.init()
        self.width, self.height = 1200, 800
        self.screen = pygame.display.set_mode((self.width, self.height), RESIZABLE)
        pygame.display.set_caption("Stardust 美学浏览器")
        self.clock = pygame.time.Clock()
        self.font_name = pygame.font.match_font('Consolas') or pygame.font.get_default_font()
        self.font_size = 18
        self.cell_size = 24
        self.min_cell_size = 10
        self.max_cell_size = 40

        self.offset_x = 20
        self.offset_y = 20
        self.dragging = False
        self.drag_start = (0, 0)
        self.drag_offset_start = (0, 0)

        if source_lines is None:
            source_lines = [
                "            +               +  *       +* +,",
                "         +            +  *      +** +,            +* +, +,",
                "        +* +,         +             +  * +,        +  *",
                "              + * +,    + +,        +* +,           + * +,",
                "             + *,        +               +  *        +*,",
                ""
            ]
        self.source_lines = source_lines
        self.grid, self.cols, self.rows = parse_source(source_lines)

        self.mouse_grid_pos = None
        self.status_text = ""
        self.show_grid = False
        self.show_leading_bg = False

        self.font = pygame.font.Font(self.font_name, self.font_size)
        self.update_font()

    def update_font(self):
        size = max(10, int(self.cell_size * 0.75))
        self.font = pygame.font.Font(self.font_name, size)

    def handle_events(self):
        for event in pygame.event.get():
            if event.type == QUIT:
                return False
            elif event.type == VIDEORESIZE:
                self.width, self.height = event.size
                self.screen = pygame.display.set_mode((self.width, self.height), RESIZABLE)
            elif event.type == MOUSEBUTTONDOWN:
                if event.button == 1:
                    self.dragging = True
                    self.drag_start = event.pos
                    self.drag_offset_start = (self.offset_x, self.offset_y)
                elif event.button == 4:
                    self.cell_size = min(self.max_cell_size, self.cell_size + 2)
                    self.update_font()
                elif event.button == 5:
                    self.cell_size = max(self.min_cell_size, self.cell_size - 2)
                    self.update_font()
            elif event.type == MOUSEBUTTONUP:
                if event.button == 1:
                    self.dragging = False
            elif event.type == MOUSEMOTION:
                if self.dragging:
                    dx = event.pos[0] - self.drag_start[0]
                    dy = event.pos[1] - self.drag_start[1]
                    self.offset_x = self.drag_offset_start[0] + dx
                    self.offset_y = self.drag_offset_start[1] + dy
                mx, my = event.pos
                col = int((mx - self.offset_x) // self.cell_size)
                row = int((my - self.offset_y) // self.cell_size)
                if 0 <= row < self.rows and 0 <= col < len(self.grid[row]):
                    self.mouse_grid_pos = (row, col)
                    cell = self.grid[row][col]
                    leading = count_leading_spaces(self.grid[row], col)
                    self.status_text = get_cell_hint(cell, leading)
                else:
                    self.mouse_grid_pos = None
                    self.status_text = ""
            elif event.type == KEYDOWN:
                if event.key == K_g:
                    self.show_grid = not self.show_grid
                elif event.key == K_b:
                    self.show_leading_bg = not self.show_leading_bg
                elif event.key == K_r:
                    self.reset_view()
        return True

    def reset_view(self):
        self.offset_x = 20
        self.offset_y = 20
        self.cell_size = 24
        self.update_font()

    def draw_grid(self):
        if not self.show_grid:
            return
        start_row = max(0, int(-self.offset_y // self.cell_size))
        end_row = min(self.rows, int((self.height - self.offset_y) // self.cell_size) + 1)
        for r in range(start_row, end_row + 1):
            y = self.offset_y + r * self.cell_size
            pygame.draw.line(self.screen, COLORS['grid_line'], (0, y), (self.width, y), 1)
        start_col = max(0, int(-self.offset_x // self.cell_size))
        end_col = min(self.cols, int((self.width - self.offset_x) // self.cell_size) + 1)
        for c in range(start_col, end_col + 1):
            x = self.offset_x + c * self.cell_size
            pygame.draw.line(self.screen, COLORS['grid_line'], (x, 0), (x, self.height), 1)

    def draw_cells(self):
        start_row = max(0, int(-self.offset_y // self.cell_size) - 1)
        end_row = min(self.rows, int((self.height - self.offset_y) // self.cell_size) + 2)

        if self.show_leading_bg:
            for r in range(start_row, end_row):
                row = self.grid[r]
                leading_end = 0
                while leading_end < len(row) and row[leading_end]['char'] == ' ':
                    leading_end += 1
                if leading_end > 0:
                    x = self.offset_x
                    y = self.offset_y + r * self.cell_size
                    rect = pygame.Rect(x, y, leading_end * self.cell_size, self.cell_size)
                    s = pygame.Surface((rect.width, rect.height), pygame.SRCALPHA)
                    s.fill(COLORS['leading_space_bg'])
                    self.screen.blit(s, rect)

        for r in range(start_row, end_row):
            y = self.offset_y + r * self.cell_size
            row = self.grid[r]
            start_col = max(0, int(-self.offset_x // self.cell_size) - 1)
            end_col = min(len(row), int((self.width - self.offset_x) // self.cell_size) + 2)
            for c in range(start_col, end_col):
                cell = row[c]
                x = self.offset_x + c * self.cell_size
                ch = cell['char']
                t = cell['type']

                if self.mouse_grid_pos == (r, c):
                    s = pygame.Surface((self.cell_size, self.cell_size), pygame.SRCALPHA)
                    s.fill(COLORS['highlight'])
                    self.screen.blit(s, (x, y))

                if ch == ' ':
                    cx = x + self.cell_size // 2
                    cy = y + self.cell_size // 2
                    radius = max(2, self.cell_size // 8)
                    dot_surf = pygame.Surface((radius*2, radius*2), pygame.SRCALPHA)
                    pygame.draw.circle(dot_surf, COLORS['space_dot'], (radius, radius), radius)
                    self.screen.blit(dot_surf, (cx - radius, cy - radius))
                else:
                    if t == 'plus':
                        color = COLORS['symbol_plus']
                    elif t == 'star':
                        color = COLORS['symbol_star']
                    elif t in ('backtick', 'quote'):
                        color = COLORS['symbol_backtick']
                    elif t in ('colon', 'semicolon'):
                        color = COLORS['symbol_colon']
                    elif t in ('dot', 'comma'):
                        color = COLORS['symbol_dot']
                    elif t == 'invalid':
                        color = COLORS['invalid']
                    else:
                        color = (200, 200, 200)
                    text_surf = self.font.render(ch, True, color)
                    text_rect = text_surf.get_rect()
                    text_rect.center = (x + self.cell_size // 2, y + self.cell_size // 2)
                    self.screen.blit(text_surf, text_rect)

    def draw_status_bar(self):
        bar_height = 30
        rect = pygame.Rect(0, self.height - bar_height, self.width, bar_height)
        pygame.draw.rect(self.screen, COLORS['status_bar_bg'], rect)

        # 固定字体大小
        status_font = pygame.font.Font(self.font_name, 18)

        if self.mouse_grid_pos:
            r, c = self.mouse_grid_pos
            left_text = f"[{r}:{c}] {self.status_text}"
        else:
            left_text = "鼠标悬停在网格上查看指令说明"

        left_surf = status_font.render(left_text, True, COLORS['status_bar_text'])
        self.screen.blit(left_surf, (10, self.height - bar_height + 5))

        hint = "拖拽平移 | 滚轮缩放 | G:网格切换 | B:行首背景切换 | R:重置视图"
        hint_surf = status_font.render(hint, True, COLORS['status_bar_text'])
        self.screen.blit(hint_surf, (self.width - hint_surf.get_width() - 10, self.height - bar_height + 5))

    def run(self):
        running = True
        while running:
            running = self.handle_events()
            self.screen.fill(COLORS['background'])
            self.draw_cells()
            self.draw_grid()
            self.draw_status_bar()
            pygame.display.flip()
            self.clock.tick(60)
        pygame.quit()
        sys.exit()

if __name__ == "__main__":
    if len(sys.argv) > 1:
        try:
            with open(sys.argv[1], 'r', encoding='utf-8') as f:
                lines = f.readlines()
        except Exception as e:
            print(f"无法读取文件: {e}")
            lines = None
    else:
        lines = None
    viewer = StardustViewer(lines)
    viewer.run()