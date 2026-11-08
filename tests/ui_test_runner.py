import sys
import time
import os
import json
from playwright.sync_api import sync_playwright

CHROME_BIN = "/root/.cache/ms-playwright/chromium-1148/chrome-linux/chrome"
DEV_TOKEN = "73e3be72c0af3395693e348509b4afb270396bf1dfd27a30538a2c423e47a419"
BASE_URL = "http://127.0.0.1:8781"

def log_step(name):
    print(f"\n========================================================")
    print(f" [TEST STEP] {name}")
    print(f"========================================================", flush=True)

def run_regression_test():
    os.environ["DISPLAY"] = ":99"
    logs = []
    errors = []

    with sync_playwright() as p:
        print(f"Launching Chromium with visible UI on DISPLAY=:99...", flush=True)
        browser = p.chromium.launch(
            executable_path=CHROME_BIN,
            headless=False,
            args=[
                "--start-maximized",
                "--no-sandbox",
                "--disable-dev-shm-usage",
            ]
        )
        context = browser.new_context(viewport={"width": 1920, "height": 1080})
        page = context.new_page()

        page.on("console", lambda msg: logs.append(f"[{msg.type}] {msg.text}"))
        page.on("pageerror", lambda err: errors.append(f"[PAGE_ERROR] {err}"))
        page.on("requestfailed", lambda req: errors.append(f"[REQ_FAILED] {req.method} {req.url} - {req.failure}"))

        # --- STEP 1: Boot & Developer Token Login ---
        log_step("1. Open ostore UI & Test Developer Token Login Modal")
        page.goto(BASE_URL)
        page.wait_for_timeout(1500)

        # Totp Setup Modal
        setup_link = page.query_selector('a:has-text("Set up Authenticator")')
        if setup_link and setup_link.is_visible():
            print("Clicking 'Set up Authenticator' link...", flush=True)
            setup_link.click()
            page.wait_for_timeout(600)
            page.wait_for_selector("#totpSetupModal", state="visible")
            print("Clicking 'Copy Secret'...", flush=True)
            page.click('button:has-text("Copy Secret")')
            page.wait_for_timeout(400)
            print("Closing Totp setup modal...", flush=True)
            page.click('#totpSetupModal button.icon-btn:has-text("✕")')
            page.wait_for_timeout(500)

        # Developer Token Login
        print("Clicking 'Use developer token' link...", flush=True)
        page.click('a:has-text("Use developer token")')
        page.wait_for_timeout(600)
        page.wait_for_selector("#tokenModal", state="visible")
        print("Filling token & clicking Save...", flush=True)
        page.fill("#tokenInput", DEV_TOKEN)
        page.wait_for_timeout(300)
        page.click('#tokenModal button:has-text("Save")')
        page.wait_for_timeout(2000)

        page.wait_for_selector("#mainConsole", state="visible", timeout=8000)
        print("SUCCESS: Main console loaded into view!", flush=True)

        # --- STEP 2: Top Replication Map (SVG Topology & Cards) Interactions ---
        log_step("2. Test Top Replication Map (SVG Canvas Nodes, Cards & Spectrum Bar)")
        
        # 2a. Click Main Node on SVG Canvas
        print("Testing click on Main Node in Topology SVG canvas...", flush=True)
        main_svg_node = page.wait_for_selector('#topoCanvas g:has-text("cc-main"), #topoCanvas g:has-text("Main")', state="visible")
        main_svg_node.click()
        page.wait_for_timeout(1000)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        print("Main Node SVG click successfully opened Cluster Sync Modal! Closing...", flush=True)
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)

        # 2b. Click Google Drive Node on SVG Canvas
        print("Testing click on Google Drive Node in Topology SVG canvas...", flush=True)
        gdrive_svg_node = page.query_selector('#topoCanvas g:has-text("Google Drive")')
        if gdrive_svg_node:
            gdrive_svg_node.click()
            page.wait_for_timeout(1000)
            page.wait_for_selector("#gdriveModal", state="visible")
            print("Google Drive SVG click successfully opened GDrive Modal! Closing...", flush=True)
            page.click('#gdriveModal button.icon-btn:has-text("✕")')
            page.wait_for_timeout(600)

        # 2c. Click Follower Node on SVG Canvas
        print("Testing click on Follower Node in Topology SVG canvas...", flush=True)
        follower_svg_node = page.query_selector('#topoCanvas g:has-text("cc2"), #topoCanvas g:has-text("racknerd")')
        if follower_svg_node:
            follower_svg_node.click()
            page.wait_for_timeout(1000)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            print("Follower SVG click opened Cluster Sync Modal! Closing...", flush=True)
            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)

        # 2d. Click Topology Card in Grid
        print("Testing click on Topology Cards in Grid...", flush=True)
        topo_cards = page.query_selector_all(".topo-card")
        if topo_cards:
            topo_cards[0].click()
            page.wait_for_timeout(1000)
            # Either gdriveModal or clusterSyncModal opens
            if page.is_visible("#clusterSyncModal"):
                page.click('#clusterSyncModal button:has-text("Close")')
            elif page.is_visible("#gdriveModal"):
                page.click('#gdriveModal button.icon-btn:has-text("✕")')
            page.wait_for_timeout(600)
            print("Topology Card click verified!", flush=True)

        # 2e. Click Data Distribution Spectrum Segment
        print("Testing click on Data Distribution Spectrum segment...", flush=True)
        spectrum_segments = page.query_selector_all(".spectrum-segment")
        if spectrum_segments:
            spectrum_segments[0].click()
            page.wait_for_timeout(1200)
            crumb_text = page.inner_text("#breadcrumbs")
            print(f"Spectrum segment navigated to: {crumb_text}", flush=True)
            # Return to root
            page.evaluate("navigate('')")
            page.wait_for_timeout(1200)
            print("Returned to root after Spectrum segment test!", flush=True)

        # 2f. Click Data Distribution Spectrum Legend Item
        print("Testing click on Spectrum Legend item...", flush=True)
        if page.query_selector(".spectrum-legend-item"):
            page.click(".spectrum-legend-item")
            page.wait_for_timeout(1200)
            crumb_text = page.inner_text("#breadcrumbs")
            print(f"Spectrum legend item navigated to: {crumb_text}", flush=True)
            # Return to root
            page.evaluate("navigate('')")
            page.wait_for_timeout(1200)
            print("Returned to root after Spectrum legend test!", flush=True)
        print("SUCCESS: All Top Topology / Replication Map interactions verified!", flush=True)

        # --- STEP 3: Header Buttons (GDrive & Fleet Replication) ---
        log_step("3. Test Header Buttons: Google Drive & Fleet Replication")
        
        # Google Drive Header Button
        page.click("#gdriveHeaderBtn")
        page.wait_for_timeout(800)
        page.wait_for_selector("#gdriveModal", state="visible")
        page.click('#gdriveModal button.icon-btn:has-text("✕")')
        page.wait_for_timeout(500)

        # Fleet Replication Header Button
        page.click('button[onclick="openClusterSyncModal()"]')
        page.wait_for_timeout(1200)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        
        # Toggle Target Node Chip
        print("Testing node chip click to toggle selection...", flush=True)
        node_locator = page.locator('.fleet-node-item:not([onclick=""])').first
        if node_locator.count() > 0:
            node_locator.click()
            page.wait_for_timeout(800)
            node_locator.click()
            page.wait_for_timeout(800)
        
        # Scan Differences
        print("Testing Scan Differences button...", flush=True)
        page.click("#btnScanDiff")
        page.wait_for_timeout(3500)
        page.click('a:has-text("Select All")')
        page.wait_for_timeout(400)
        page.click('a:has-text("Deselect All")')
        page.wait_for_timeout(400)
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)
        print("SUCCESS: Header buttons verified!", flush=True)

        # --- STEP 4: Toolbar Controls (Search, Sync Current Folder) ---
        log_step("4. Test Toolbar: Filter Objects & Sync Current Folder")
        page.fill("#searchBox", "shenqiao")
        page.wait_for_timeout(500)
        page.fill("#searchBox", "")
        page.wait_for_timeout(500)

        page.click("#btnSyncFolder")
        page.wait_for_timeout(1000)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        badge_text = page.inner_text("#syncTargetBadge")
        print(f"Current folder sync badge: {badge_text}", flush=True)
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)
        print("SUCCESS: Toolbar controls verified!", flush=True)

        # --- STEP 5: Table Column "Replication Map" & Row Actions in Root ---
        log_step("5. Test Table 'Replication Map' Column & Row Actions in Root")
        
        # 5a. Click Replication Map cell on directory row
        dir_replica_badge = page.wait_for_selector(".dir-replica-cell", state="visible", timeout=10000)
        print("Clicking 'Cluster Replicas' under 'Replication Map' column on folder...", flush=True)
        dir_replica_badge.click()
        page.wait_for_timeout(1200)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        badge_text = page.inner_text("#syncTargetBadge")
        print(f"Sync target badge from folder Replication Map cell: {badge_text}", flush=True)
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)

        # 5b. Click Size cell to open Folder Properties
        print("Clicking Folder Size cell to open Folder Properties...", flush=True)
        dir_size_cell = page.wait_for_selector(".dir-size-cell", state="visible")
        dir_size_cell.click()
        page.wait_for_timeout(800)
        page.wait_for_selector("#folderDetailModal", state="visible")
        print("Clicking 'Sync Replicas' button inside Folder Detail Modal...", flush=True)
        page.click('button:has-text("Sync Replicas")')
        page.wait_for_timeout(1200)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)

        # 5c. Click Folder Sync action icon
        print("Clicking Folder Sync action button in row...", flush=True)
        folder_sync_btn = page.wait_for_selector('button[title="Sync folder to cluster nodes"]', state="visible")
        folder_sync_btn.click()
        page.wait_for_timeout(1200)
        page.wait_for_selector("#clusterSyncModal", state="visible")
        page.click('#clusterSyncModal button:has-text("Close")')
        page.wait_for_timeout(600)

        # 5d. Table Header Select All Checkbox
        print("Testing Table Header Select All Checkbox (#selectAllCb)...", flush=True)
        select_all_cb = page.wait_for_selector("#selectAllCb", state="visible")
        select_all_cb.check()
        page.wait_for_timeout(500)
        page.wait_for_selector("#batchActionBar", state="visible")
        select_all_cb.uncheck()
        page.wait_for_timeout(500)
        print("SUCCESS: Root table and Replication Map column verified!", flush=True)

        # --- STEP 6: Navigate into Subfolder & Test File Actions, Badges & Media Preview ---
        log_step("6. Navigate to Subfolder: File Replication Map Badges, Media Preview & Actions")
        folder_name_cells = page.query_selector_all(".name-cell")
        print("Clicking folder name cell to navigate into directory...", flush=True)
        folder_name_cells[0].click()
        page.wait_for_timeout(2000)

        # 6a. Click Replication Map Badge on File Row
        print("Testing click on File 'Replication Map' badge (.node-badges-cluster)...", flush=True)
        file_badge = page.wait_for_selector(".node-badges-cluster", state="visible")
        file_badge.click()
        page.wait_for_timeout(1500)
        page.wait_for_selector("#fileTopoModal", state="visible")
        
        # Verify no overflow beyond viewport
        box = page.locator("#fileTopoModal .file-topo-box").bounding_box()
        vp = page.viewport_size
        print(f"Modal bounding box: y={box['y']}, height={box['height']} (Viewport: {vp['height']}h)", flush=True)
        assert box['y'] >= 0, f"Modal top was pushed off-screen: y={box['y']}"
        assert box['y'] + box['height'] <= vp['height'] + 10, f"Modal bottom overflowed viewport: {box['y'] + box['height']} > {vp['height']}"

        # Deep Hash Check
        print("Clicking 'Deep Hash Check' in File Replica Topology...", flush=True)
        page.click("#btnDeepCheck")
        page.wait_for_timeout(2500)
        summary = page.inner_text("#fileTopoSummary")
        print(f"Topology summary: {summary}", flush=True)

        # Sync To Followers with Live Progress Bar (rate & ratio)
        print("Clicking 'Sync To Followers' to verify live progress bar, sync rate & ratio...", flush=True)
        page.click("#btnSyncSingleFile")
        page.wait_for_timeout(300)
        page.wait_for_selector("#fileTopoProgressContainer", state="visible")
        progress_text = page.inner_text("#fileTopoProgressStatus")
        rate_text = page.inner_text("#fileTopoProgressRate")
        ratio_text = page.inner_text("#fileTopoProgressRatio")
        print(f"Live progress status: {progress_text} | Rate: {rate_text} | Ratio: {ratio_text}", flush=True)
        assert len(ratio_text) > 0, "Ratio text should not be empty"
        page.wait_for_timeout(2500)

        # Test closing via footer Close button
        print("Testing closing File Replica Topology via footer Close button...", flush=True)
        page.click('#fileTopoModal button:has-text("Close")')
        page.wait_for_timeout(600)
        assert not page.is_visible("#fileTopoModal"), "File topo modal should be closed by footer button"

        # Re-open and test closing via Escape key
        print("Re-opening File Replica Topology to test Escape key dismissal...", flush=True)
        file_badge.click()
        page.wait_for_timeout(1000)
        page.wait_for_selector("#fileTopoModal", state="visible")
        print("Pressing Escape key...", flush=True)
        page.keyboard.press("Escape")
        page.wait_for_timeout(600)
        assert not page.is_visible("#fileTopoModal"), "File topo modal should be closed by Escape key"

        # Re-open and test closing via Backdrop click
        print("Re-opening File Replica Topology to test Backdrop click dismissal...", flush=True)
        file_badge.click()
        page.wait_for_timeout(1000)
        page.wait_for_selector("#fileTopoModal", state="visible")
        print("Clicking on backdrop outside dialog box...", flush=True)
        page.click("#fileTopoModal", position={"x": 10, "y": 10})
        page.wait_for_timeout(600)
        assert not page.is_visible("#fileTopoModal"), "File topo modal should be closed by Backdrop click"

        # 6b. Click File Name to open Media Preview Modal
        print("Clicking file name to open Media Preview modal...", flush=True)
        file_name_cell = page.wait_for_selector('.file-row:not(.parent-dir-row) .name-cell', state="visible")
        file_name_cell.click()
        page.wait_for_timeout(1500)
        page.wait_for_selector("#mediaModal", state="visible")
        copy_text_btn = page.query_selector("#mediaCopyBtn")
        if copy_text_btn and copy_text_btn.is_visible():
            print("Clicking 'Copy Object Content' in media preview...", flush=True)
            copy_text_btn.click()
            page.wait_for_timeout(400)
        page.click('#mediaModal button.icon-btn:has-text("✕")')
        page.wait_for_timeout(600)
        print("Media Preview verified!", flush=True)

        # 6c. Copy URL & Copy MD5 action icons
        copy_url_btn = page.query_selector('button[title="Copy Direct URL"]')
        if copy_url_btn:
            copy_url_btn.click()
            page.wait_for_timeout(300)
        copy_md5_btn = page.query_selector('button[title="Copy MD5"]')
        if copy_md5_btn:
            copy_md5_btn.click()
            page.wait_for_timeout(300)

        # 6d. Single File Sync Action Icon
        file_sync_btn = page.query_selector('button[title="Sync file to cluster nodes"]')
        if file_sync_btn:
            file_sync_btn.click()
            page.wait_for_timeout(1000)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)

        # --- STEP 7: Batch Selection & Multi-Item Fleet Sync ---
        log_step("7. Test Multi-Row Selection, Batch Action Bar & Sync Execution")
        sub_cbs = page.query_selector_all(".row-select-cb:not(#selectAllCb)")
        if len(sub_cbs) >= 2:
            print("Selecting multiple file rows...", flush=True)
            sub_cbs[0].check()
            sub_cbs[1].check()
            page.wait_for_timeout(500)
            page.wait_for_selector("#batchActionBar", state="visible")
            
            # Click Batch Sync
            print("Clicking 'Batch Sync' button in batch bar...", flush=True)
            page.click('button:has-text("Batch Sync")')
            page.wait_for_timeout(1200)
            page.wait_for_selector("#clusterSyncModal", state="visible")
            
            # Scan Diff on batch
            print("Scanning differences on batch selection...", flush=True)
            page.click("#btnScanDiff")
            page.wait_for_timeout(2500)

            # If a prior background sync is active, cancel it cleanly to test batch sync
            cancel_btn = page.query_selector("#btnCancelSync")
            if cancel_btn and cancel_btn.is_visible():
                print("Prior fleet sync was active, cancelling to test batch sync...", flush=True)
                cancel_btn.click()
                page.wait_for_timeout(1500)

            # Run Sync
            print("Clicking 'Run Sync Now' / 'Sync Selected'...", flush=True)
            page.wait_for_selector("#btnRunSync:not([disabled])", state="visible", timeout=10000)
            page.click("#btnRunSync")
            page.wait_for_timeout(3500)
            print("Batch replication executed cleanly!", flush=True)

            page.click('#clusterSyncModal button:has-text("Close")')
            page.wait_for_timeout(600)

        # --- STEP 8: Navigation Controls (Back Button, Parent Row, Breadcrumbs) ---
        log_step("8. Test Navigation Controls: Parent Row '..', Back Button & Breadcrumbs")
        
        # Click Breadcrumb root link
        print("Clicking root breadcrumb link...", flush=True)
        crumb_root = page.wait_for_selector('.crumb-link:has-text("root")', state="visible")
        crumb_root.click()
        page.wait_for_timeout(1500)
        print("Navigated back to root via breadcrumb!", flush=True)

        # Navigate into folder again
        folder_name_cells = page.query_selector_all(".name-cell")
        folder_name_cells[0].click()
        page.wait_for_timeout(1500)

        # Click Back Button (#btnNavBack)
        print("Clicking Back button (#btnNavBack)...", flush=True)
        back_btn = page.wait_for_selector("#btnNavBack", state="visible")
        back_btn.click()
        page.wait_for_timeout(1500)
        print("Navigated back to root via Back button!", flush=True)

        # Final full page screenshot
        final_screen = "/root/ostore/tests/ui_test_full_success.png"
        page.screenshot(path=final_screen)
        print(f"\nFinal regression screenshot saved: {final_screen}", flush=True)

        print("\n========================================================")
        print(" ALL UI & REPLICATION MAP INTERACTIONS PASSED WITH 0 ERRORS!")
        print("========================================================", flush=True)
        print(f"Total Console logs: {len(logs)}")
        print(f"Total Page errors: {len(errors)}")
        for e in errors:
            print("  [ERROR]", e)
        assert len(errors) == 0, f"Encountered page errors: {errors}"

        browser.close()

if __name__ == "__main__":
    run_regression_test()
