"""Launch the STL viewer GUI: python run_viewer.py [files...]"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))  # works from any working directory

from stl_viewer.gui.app import main  # noqa: E402

if __name__ == "__main__":
    sys.exit(main())
