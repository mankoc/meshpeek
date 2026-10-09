from dataclasses import replace

from PySide6.QtGui import QColor
from PySide6.QtWidgets import QCheckBox, QColorDialog, QComboBox, QDialog, QDialogButtonBox, QFormLayout, QPushButton

from .settings import MATERIALS, SUPPORT_DETAIL, ViewSettings


class SettingsDialog(QDialog):
    """Edits a copy of the settings; every change is previewed through `on_change`, Cancel restores."""

    def __init__(self, settings: ViewSettings, on_change, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Preferences")
        self._original = settings
        self.settings = replace(settings)
        self._on_change = on_change

        self.support_detail = QComboBox()
        self.support_detail.addItems(SUPPORT_DETAIL)
        self.support_detail.setCurrentText(settings.support_detail)
        self.support_detail.setToolTip(
            "Smoothness of Lychee supports (column facets and joints). Applied by reloading open files."
        )
        self.support_detail.currentTextChanged.connect(lambda name: setattr(self.settings, "support_detail", name))

        self.obj_y_up = QCheckBox("Y is up (Blender, Maya, ZBrush)")
        self.obj_y_up.setChecked(settings.obj_y_up)
        self.obj_y_up.setToolTip("Rotate OBJ files so their Y axis points up. Applied by reloading open files.")
        self.obj_y_up.toggled.connect(lambda on: setattr(self.settings, "obj_y_up", on))

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        form = QFormLayout(self)
        form.addRow("Model material:", self._material_box("material"))
        form.addRow("Model colour:", self._color_button("model_color", "Model colour (when the file has none)"))
        form.addRow("Support material:", self._material_box("support_material"))
        form.addRow("Support colour:", self._color_button("support_color", "Support colour"))
        form.addRow("Support detail:", self.support_detail)
        form.addRow("OBJ files:", self.obj_y_up)
        form.addRow(buttons)

    def _set(self, key: str, value):
        setattr(self.settings, key, value)
        self._on_change(self.settings)

    def _material_box(self, key: str) -> QComboBox:
        box = QComboBox()
        box.addItems(MATERIALS)
        box.setCurrentText(getattr(self.settings, key))
        box.currentTextChanged.connect(lambda name: self._set(key, name))
        return box

    def _color_button(self, key: str, title: str) -> QPushButton:
        button = QPushButton()

        def show():
            color = getattr(self.settings, key)
            text = "#000" if QColor(color).lightness() > 128 else "#fff"
            button.setText(color)
            button.setStyleSheet(f"background-color: {color}; color: {text};")

        def pick():
            color = QColorDialog.getColor(QColor(getattr(self.settings, key)), self, title)
            if color.isValid():
                self._set(key, color.name())
                show()

        button.clicked.connect(pick)
        show()
        return button

    def reject(self):
        self._on_change(self._original)
        super().reject()
