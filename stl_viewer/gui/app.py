import sys

from PySide6.QtWidgets import QApplication

from .main_window import MainWindow


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv if argv is None else argv
    app = QApplication(argv)
    window = MainWindow()
    window.show()
    window.open_files(argv[1:])
    return app.exec()
