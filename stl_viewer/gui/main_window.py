import time
from pathlib import Path

from PySide6.QtCore import QObject, QRunnable, Qt, QThreadPool, Signal
from PySide6.QtGui import QAction, QColor, QIcon, QKeySequence, QPainter, QPen, QPixmap
from PySide6.QtWidgets import (
    QDockWidget, QFileDialog, QHeaderView, QMainWindow, QMenu, QStyle, QTreeWidget, QTreeWidgetItem
)

from ..core.io import SUPPORTED_EXTENSIONS, LoadOptions, load
from ..core.scene import Scene
from .settings import ViewSettings, add_recent_file, clear_recent_files, recent_files
from .settings_dialog import SettingsDialog
from .viewport import Viewport

CENTER_COLUMN = 1  # narrow column showing the "centred on this object" icon


def _target_icon(size: int = 16) -> QIcon:
    """Small crosshair drawn at runtime, so no image files are needed."""
    pixmap = QPixmap(size, size)
    pixmap.fill(Qt.GlobalColor.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    painter.setPen(QPen(QColor("#4fa3ff"), 1.6))
    painter.drawEllipse(2, 2, size - 4, size - 4)
    painter.drawLine(size // 2, 0, size // 2, 4)
    painter.drawLine(size // 2, size - 4, size // 2, size)
    painter.drawLine(0, size // 2, 4, size // 2)
    painter.drawLine(size - 4, size // 2, size, size // 2)
    painter.setBrush(QColor("#4fa3ff"))
    painter.drawEllipse(size // 2 - 2, size // 2 - 2, 4, 4)
    painter.end()
    return QIcon(pixmap)


FILE_FILTER = "3D files ({});;All files (*)".format(" ".join(f"*{e}" for e in SUPPORTED_EXTENSIONS))


class _LoadSignals(QObject):
    done = Signal(str, object, float)
    failed = Signal(str, str)


class _LoadTask(QRunnable):
    """Parse a file off the UI thread; only the actor creation happens on the UI thread."""

    def __init__(self, path: str, options: LoadOptions):
        super().__init__()
        self.path = path
        self.options = options
        self.signals = _LoadSignals()

    def run(self):
        start = time.perf_counter()
        try:
            scene = load(self.path, self.options)
        except Exception as e:
            self.signals.failed.emit(self.path, str(e))
        else:
            self.signals.done.emit(self.path, scene, time.perf_counter() - start)


class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("STL Viewer")
        self.resize(1400, 900)
        self.setAcceptDrops(True)

        self.settings = ViewSettings.load()
        self.viewport = Viewport(self.settings, self)
        self.setCentralWidget(self.viewport)

        self.object_list = QTreeWidget()
        self.object_list.setHeaderHidden(True)
        self.object_list.setColumnCount(2)
        header = self.object_list.header()
        header.setStretchLastSection(False)
        header.setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        header.setSectionResizeMode(CENTER_COLUMN, QHeaderView.ResizeMode.Fixed)
        header.resizeSection(CENTER_COLUMN, 24)
        self.object_list.itemClicked.connect(self._on_item_clicked)
        self._centered_item: QTreeWidgetItem | None = None
        self._target_icon = _target_icon()
        self.object_list.itemChanged.connect(self._on_item_changed)
        self.object_list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.object_list.customContextMenuRequested.connect(self._object_menu)
        dock = QDockWidget("Objects", self)
        dock.setWidget(self.object_list)
        self.addDockWidget(Qt.DockWidgetArea.LeftDockWidgetArea, dock)

        self._tasks: set[_LoadTask] = set()  # keep tasks (and their signal objects) alive until they report
        self._has_content = False
        self._loaded_paths: list[str] = []  # in load order, for reloading when load options change
        self._build_menus(dock)
        self.statusBar().showMessage("Open or drop STL, OBJ, PLY or LYS files")

    def _build_menus(self, dock: QDockWidget):
        def action(menu, text, slot, shortcut=None, checkable=False):
            act = QAction(text, self, checkable=checkable)
            if shortcut:
                act.setShortcut(QKeySequence(shortcut))
            act.triggered.connect(slot)
            menu.addAction(act)
            return act

        file_menu = self.menuBar().addMenu("&File")
        open_act = action(file_menu, "&Open…", self._open_dialog, QKeySequence.StandardKey.Open)
        self.recent_menu = file_menu.addMenu("Open &Recent")
        self.recent_menu.setToolTipsVisible(True)
        self.recent_menu.aboutToShow.connect(self._fill_recent_menu)
        clear_act = action(file_menu, "&Clear scene", self.clear, "Ctrl+W")
        open_act.setIcon(self.style().standardIcon(QStyle.StandardPixmap.SP_DialogOpenButton))
        clear_act.setIcon(self.style().standardIcon(QStyle.StandardPixmap.SP_TrashIcon))

        toolbar = self.addToolBar("Main")
        toolbar.setObjectName("main_toolbar")
        toolbar.setMovable(False)
        toolbar.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon)
        toolbar.addActions([open_act, clear_act])
        file_menu.addSeparator()
        action(file_menu, "E&xit", self.close, QKeySequence.StandardKey.Quit)

        vp = self.viewport
        view_menu = self.menuBar().addMenu("&View")
        action(view_menu, "&Fit all", lambda: self.center_on(None), "F")
        action(view_menu, "&Isometric", lambda: vp.show_view(vp.view_isometric), "0")
        for text, slot, key in [
            ("Front", lambda: vp.show_view(vp.view_xz), "1"),
            ("Back", lambda: vp.show_view(vp.view_xz, negative=True), "2"),
            ("Left", lambda: vp.show_view(vp.view_yz, negative=True), "3"),
            ("Right", lambda: vp.show_view(vp.view_yz), "4"),
            ("Top", lambda: vp.show_view(vp.view_xy), "5"),
            ("Bottom", lambda: vp.show_view(vp.view_xy, negative=True), "6"),
        ]:
            action(view_menu, text, slot, key)
        view_menu.addSeparator()
        action(view_menu, "&Wireframe", vp.set_wireframe, "W", checkable=True)
        view_menu.addAction(dock.toggleViewAction())

        settings_menu = self.menuBar().addMenu("&Settings")
        action(settings_menu, "&Preferences…", self._open_settings, "Ctrl+,")

    def _fill_recent_menu(self):
        menu = self.recent_menu
        menu.clear()
        files = recent_files()
        for number, path in enumerate(files, 1):
            act = menu.addAction(f"&{number % 10}  {Path(path).name}  —  {Path(path).parent}")
            act.setToolTip(path)
            act.setEnabled(Path(path).exists())
            act.triggered.connect(lambda _=False, p=path: self.open_files([p]))
        if not files:
            menu.addAction("No recent files").setEnabled(False)
            return
        menu.addSeparator()
        menu.addAction("&Clear list", clear_recent_files)

    # --- loading -------------------------------------------------------------------------------

    def open_files(self, paths):
        for path in paths:
            task = _LoadTask(str(path), self.settings.load_options())
            task.signals.done.connect(self._on_loaded)
            task.signals.failed.connect(self._on_failed)
            self._tasks.add(task)
            QThreadPool.globalInstance().start(task)
            self.statusBar().showMessage(f"Loading {Path(path).name}…")

    def _finish_task(self, path: str):
        self._tasks = {t for t in self._tasks if t.path != path}

    def _on_loaded(self, path: str, scene: Scene, seconds: float):
        self._finish_task(path)
        add_recent_file(path)
        self._loaded_paths.append(path)
        first = not self._has_content
        for plate in scene.plates:
            self.viewport.add_plate(plate)
        items = {}  # id(SceneObject) -> its tree item, so children can find their parent
        for obj in scene.objects:
            self.viewport.add_object(obj)
            parent_item = items.get(id(obj.parent)) if obj.parent is not None else None
            item = QTreeWidgetItem(parent_item or self.object_list, [obj.name])
            item.setFlags(item.flags() | Qt.ItemFlag.ItemIsUserCheckable)
            item.setCheckState(0, Qt.CheckState.Checked if obj.visible else Qt.CheckState.Unchecked)
            item.setToolTip(0, f"{obj.source}\n{obj.mesh.n_faces:,} triangles")
            item.setData(0, Qt.ItemDataRole.UserRole, obj)
            items[id(obj)] = item
            if parent_item is not None:
                parent_item.setExpanded(True)
        for reason in scene.skipped:
            item = QTreeWidgetItem(self.object_list, [f"⚠ {reason}"])
            item.setFlags(Qt.ItemFlag.NoItemFlags)
            item.setForeground(0, QColor("#c08a3e"))
        self._has_content = self._has_content or bool(scene.objects or scene.plates)
        if first:
            self.viewport.view_isometric()
        self.viewport.render()

        triangles = sum(o.mesh.n_faces for o in scene.objects)
        msg = f"{Path(path).name}: {len(scene.objects)} objects, {triangles:,} triangles in {seconds * 1000:.0f} ms"
        if scene.skipped:
            msg += f" — {len(scene.skipped)} skipped"
        self.statusBar().showMessage(msg)

    def _on_failed(self, path: str, error: str):
        self._finish_task(path)
        self.statusBar().showMessage(f"Could not open {Path(path).name}: {error}")

    def _open_settings(self):
        dialog = SettingsDialog(self.settings, self.viewport.apply_settings, self)
        if dialog.exec():
            reload = dialog.settings.load_options() != self.settings.load_options()
            self.settings = dialog.settings
            self.settings.save()
            if reload and self._loaded_paths:
                paths = list(self._loaded_paths)
                self.clear()
                self.open_files(paths)

    def _open_dialog(self):
        paths, _ = QFileDialog.getOpenFileNames(self, "Open 3D files", "", FILE_FILTER)
        self.open_files(paths)

    def clear(self):
        self.viewport.clear_scene()
        self._centered_item = None  # the item is deleted with the list
        self.object_list.clear()
        self._has_content = False
        self._loaded_paths.clear()

    def _object_menu(self, pos):
        item = self.object_list.itemAt(pos)
        obj = item.data(0, Qt.ItemDataRole.UserRole) if item else None
        if obj is None:  # empty space or a skipped-file note
            return
        menu = QMenu(self)
        center = menu.addAction("Center on object")
        center.triggered.connect(lambda: self.center_on(item))
        menu.exec(self.object_list.viewport().mapToGlobal(pos))

    def center_on(self, item: QTreeWidgetItem | None):
        """Centre the camera on an object list item and mark it with the target icon; None frames everything."""
        if self._centered_item is not None:
            self._centered_item.setIcon(CENTER_COLUMN, QIcon())
            self._centered_item.setToolTip(CENTER_COLUMN, "")
        self._centered_item = item
        if item is None:
            self.viewport.fit()
            return
        item.setIcon(CENTER_COLUMN, self._target_icon)
        item.setToolTip(CENTER_COLUMN, "Camera is centred on this object. Click to stop centring.")
        self.viewport.center_on(item.data(0, Qt.ItemDataRole.UserRole))

    def _on_item_clicked(self, item: QTreeWidgetItem, column: int):
        if column == CENTER_COLUMN and item is self._centered_item:
            self.center_on(None)

    def _on_item_changed(self, item: QTreeWidgetItem, column: int):
        obj = item.data(0, Qt.ItemDataRole.UserRole)
        if obj is not None:
            self.viewport.set_visible(obj, item.checkState(0) == Qt.CheckState.Checked)

    # --- drag and drop -------------------------------------------------------------------------

    def dragEnterEvent(self, event):
        if event.mimeData().hasUrls():
            event.acceptProposedAction()

    def dropEvent(self, event):
        paths = [u.toLocalFile() for u in event.mimeData().urls()]
        self.open_files([p for p in paths if Path(p).suffix.lower() in SUPPORTED_EXTENSIONS])

    def closeEvent(self, event):
        self.viewport.close()
        super().closeEvent(event)
