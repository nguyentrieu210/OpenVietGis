// W0 real native library probe; a successful link is not a device result.
#include <gdal.h>
#include <ogr_api.h>
#include <ogr_srs_api.h>
#include <cpl_conv.h>
#include <proj.h>
#include <geos_c.h>
#include <sqlite3.h>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <unistd.h>

static void require(bool ok, const char *reason) {
  if (!ok) { std::fprintf(stderr, "FAIL: %s\n", reason); std::exit(1); }
}
int main(int argc, char **argv) {
  require(argc == 3, "usage: probe PROJ_DATA_DIR NEW_OUTPUT_GPKG");
  const char *dir = argv[1], *path = argv[2];
  require(access(path, F_OK) != 0, "output must not exist");
  CPLSetConfigOption("PROJ_DATA", dir);
  CPLSetConfigOption("PROJ_NETWORK", "OFF");
  const char *search[] = {dir, nullptr};
  OSRSetPROJSearchPaths(search);
  std::printf("page_size=%ld GDAL=%s PROJ=%s GEOS=%s SQLite=%s\n",
      sysconf(_SC_PAGESIZE), GDALVersionInfo("RELEASE_NAME"),
      proj_info().version, GEOSversion(), sqlite3_libversion());
  PJ_CONTEXT *ctx = proj_context_create();
  require(ctx != nullptr, "PROJ context");
  const char *paths[] = {dir};
  proj_context_set_search_paths(ctx, 1, paths);
  proj_context_set_enable_network(ctx, 0);
  PJ *raw = proj_create_crs_to_crs(ctx, "EPSG:4326", "EPSG:3857", nullptr);
  require(raw != nullptr, "PROJ offline CRS lookup");
  PJ *transform = proj_normalize_for_visualization(ctx, raw);
  require(transform != nullptr, "PROJ axis normalization");
  const PJ_COORD p = proj_trans(transform, PJ_FWD, proj_coord(1, 0, 0, 0));
  require(proj_errno(transform) == 0 && std::isfinite(p.xy.x)
      && std::abs(p.xy.x - 111319.49079327357) < 0.001
      && std::abs(p.xy.y) < 0.001, "EPSG:3857 equatorial control");
  proj_destroy(transform); proj_destroy(raw); proj_context_destroy(ctx);
  GEOSContextHandle_t geos = GEOS_init_r();
  require(geos != nullptr, "GEOS init");
  GEOSWKTReader *reader = GEOSWKTReader_create_r(geos);
  require(reader != nullptr, "GEOS reader");
  GEOSGeometry *poly = GEOSWKTReader_read_r(geos, reader,
      "POLYGON((0 0,2 0,2 2,0 2,0 0))");
  double area = 0;
  require(poly && GEOSisValid_r(geos, poly) == 1
      && GEOSArea_r(geos, poly, &area) == 1 && std::abs(area-4.0) < 1e-12, "GEOS area");
  GEOSGeom_destroy_r(geos, poly); GEOSWKTReader_destroy_r(geos, reader); GEOS_finish_r(geos);
  GDALAllRegister();
  const char *drivers[] = {"GPKG", "GeoJSON", "ESRI Shapefile", "MapInfo File", "GTiff"};
  for (const char *name: drivers) {
    require(GDALGetDriverByName(name) != nullptr, name);
    std::printf("driver=%s\n", name);
  }
  GDALDatasetH ds = GDALCreate(GDALGetDriverByName("GPKG"), path, 0, 0, 0, GDT_Unknown, nullptr);
  require(ds != nullptr, "create GeoPackage");
  OGRSpatialReferenceH srs = OSRNewSpatialReference(nullptr);
  require(srs && OSRImportFromEPSG(srs, 4326) == OGRERR_NONE, "GPKG CRS");
  OGRLayerH layer = GDALDatasetCreateLayer(ds, "probe", srs, wkbPoint, nullptr);
  require(layer != nullptr, "GPKG create layer");
  OGRFieldDefnH fld = OGR_Fld_Create("label", OFTString);
  require(fld && OGR_L_CreateField(layer, fld, 0) == OGRERR_NONE, "GPKG field");
  OGR_Fld_Destroy(fld);
  OGRFeatureH f = OGR_F_Create(OGR_L_GetLayerDefn(layer));
  require(f != nullptr, "GPKG feature");
  OGR_F_SetFieldString(f, 0, "Địa điểm");
  OGRGeometryH geom = OGR_G_CreateGeometry(wkbPoint);
  require(geom != nullptr, "GPKG geometry");
  OGR_G_SetPoint_2D(geom, 0, 106.7, 10.8);
  require(OGR_F_SetGeometryDirectly(f, geom) == OGRERR_NONE
      && OGR_L_CreateFeature(layer, f) == OGRERR_NONE, "GPKG insert");
  OGR_F_Destroy(f); GDALClose(ds); OSRDestroySpatialReference(srs);
  ds = GDALOpenEx(path, GDAL_OF_VECTOR | GDAL_OF_READONLY, nullptr, nullptr, nullptr);
  require(ds != nullptr, "GPKG reopen");
  layer = GDALDatasetGetLayerByName(ds, "probe");
  require(layer && OGR_L_GetFeatureCount(layer, 1) == 1, "GPKG feature count");
  f = OGR_L_GetNextFeature(layer);
  require(f && std::string(OGR_F_GetFieldAsString(f, 0)) == "Địa điểm", "GPKG Unicode");
  geom = OGR_F_GetGeometryRef(f);
  require(geom && std::abs(OGR_G_GetX(geom, 0)-106.7)<1e-10
      && std::abs(OGR_G_GetY(geom, 0)-10.8)<1e-10, "GPKG coordinate");
  OGR_F_Destroy(f); GDALClose(ds);
  sqlite3 *db = nullptr;
  require(sqlite3_open(":memory:", &db) == SQLITE_OK, "SQLite open");
  require(sqlite3_exec(db, "CREATE VIRTUAL TABLE idx USING rtree(id,minx,maxx,miny,maxy);"
      "INSERT INTO idx VALUES(1,1,2,3,4);", nullptr, nullptr, nullptr) == SQLITE_OK, "SQLite RTree");
  sqlite3_stmt *stmt = nullptr;
  require(sqlite3_prepare_v2(db, "SELECT id FROM idx WHERE minx<=1.5 AND maxx>=1.5", -1, &stmt, nullptr)==SQLITE_OK
      && sqlite3_step(stmt)==SQLITE_ROW && sqlite3_column_int(stmt,0)==1, "RTree query");
  sqlite3_finalize(stmt); sqlite3_close(db);
  std::puts("NATIVE_PROBE_PASS");
  return 0;
}
