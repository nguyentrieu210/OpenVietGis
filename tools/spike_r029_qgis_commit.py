"""Isolated native QGIS feasibility probe, no server or production data."""
import os
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

def main():
    from qgis.core import QgsApplication, QgsFeature, QgsGeometry, QgsPointXY, QgsVectorLayer, Qgis
    app = QgsApplication([], False)
    app.initQgis()
    try:
        layer = QgsVectorLayer("Point?crs=EPSG:4326", "r029-spike", "memory")
        assert layer.isValid()
        assert hasattr(layer, "setAllowCommit"), "Native commit veto API unavailable"
        state = {"accepted": False, "calls": 0}
        def gate(*args):
            state["calls"] += 1
            layer.setAllowCommit(state["accepted"])
        layer.beforeCommitChanges.connect(gate)
        assert layer.startEditing()
        feature = QgsFeature(layer.fields())
        feature.setGeometry(QgsGeometry.fromPointXY(QgsPointXY(106, 11)))
        assert layer.addFeature(feature)
        assert not layer.commitChanges(), "Rejected save reported success"
        assert layer.isEditable() and layer.isModified(), "Rejected save lost draft"
        assert layer.dataProvider().featureCount() == 0, "Rejected save reached provider"
        state["accepted"] = True
        assert layer.commitChanges(), "Accepted save failed"
        assert layer.dataProvider().featureCount() == 1
        assert state["calls"] == 2
        print("PASS native commit veto and retry:", Qgis.QGIS_VERSION)
        del layer
    finally:
        app.exitQgis()

if __name__ == "__main__":
    main()
