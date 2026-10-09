import pytest
from PySide6.QtCore import QSettings

from stl_viewer.gui import settings


@pytest.fixture(autouse=True)
def temp_store(tmp_path, monkeypatch):
    """Keep tests out of the user's real settings (the Windows registry)."""
    monkeypatch.setattr(settings, "_store", lambda: QSettings(str(tmp_path / "s.ini"), QSettings.Format.IniFormat))


def test_recent_files_newest_first_without_duplicates(tmp_path):
    a, b = tmp_path / "a.stl", tmp_path / "b.lys"
    settings.add_recent_file(a)
    settings.add_recent_file(b)
    settings.add_recent_file(a)
    assert settings.recent_files() == [str(a), str(b)]


def test_recent_files_single_entry_is_a_list(tmp_path):
    settings.add_recent_file(tmp_path / "only.stl")
    assert settings.recent_files() == [str(tmp_path / "only.stl")]


def test_recent_files_capped_and_clearable(tmp_path):
    for i in range(settings.MAX_RECENT + 3):
        settings.add_recent_file(tmp_path / f"{i}.stl")
    files = settings.recent_files()
    assert len(files) == settings.MAX_RECENT and files[0].endswith(f"{settings.MAX_RECENT + 2}.stl")
    settings.clear_recent_files()
    assert settings.recent_files() == []


def test_view_settings_roundtrip():
    settings.ViewSettings(material="Metal", model_color="#123456").save()
    assert settings.ViewSettings.load() == settings.ViewSettings("Metal", "#123456", "Medium", True)


def test_support_appearance_roundtrip_and_validation():
    settings.ViewSettings(support_material="Glossy", support_color="#2266cc").save()
    loaded = settings.ViewSettings.load()
    assert (loaded.support_material, loaded.support_color) == ("Glossy", "#2266cc")
    settings._store().setValue("support_material", "Chrome")  # unknown preset falls back to the default
    assert settings.ViewSettings.load().support_material == settings.ViewSettings().support_material
