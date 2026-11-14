(ns sa.ingenious.golden-test
  "Replay tests/jvm-golden: every case of the corpus the UniFFI binding produced must come out
  the same through the FFM Java API, after converting to the old map shapes. Layout and raw pages
  go through their records (`toRecords`, `rawTextBoxes`), which the page views build."
  (:require [clojure.test :refer [deftest is testing]]
            [sa.ingenious.corpus :as corpus])
  (:import [sa.ingenious.pdf BoundingBox Cursor Document LayoutOptions LayoutPage Metadata
            MetadataEntry Options PageSummary PageTableRows PageText PdfException RawPage
            RawTableBoundingBox Table TableOptions TableStrategy]))

(set! *warn-on-reflection* true)

;; ---- options: old Clojure maps to Java records -------------------------------------------

(defn- layout-options ^LayoutOptions [m]
  (let [b (LayoutOptions/builder)]
    (some->> (:line-overlap m) double (.lineOverlap b))
    (some->> (:char-margin m) double (.charMargin b))
    (some->> (:line-margin m) double (.lineMargin b))
    (some->> (:word-margin m) double (.wordMargin b))
    (when (contains? m :boxes-flow)
      (if-some [flow (:boxes-flow m)] (.boxesFlow b (double flow)) (.disableBoxesFlow b)))
    (some->> (:detect-vertical m) boolean (.detectVertical b))
    (some->> (:all-texts m) boolean (.allTexts b))
    (.build b)))

(defn- options ^Options [m]
  (let [b (Options/builder)]
    (some->> (:password m) (.password b))
    (when-some [pages (:pages m)] (.pages b ^java.util.List (mapv int pages)))
    (some->> (:max-pages m) int (.maxPages b))
    (some->> (:caching m) boolean (.caching b))
    (some->> (:bidi m) boolean (.bidi b))
    (some->> (:layout m) layout-options (.layout b))
    (.build b)))

(defn- bbox-of ^BoundingBox [[x0 y0 x1 y1]]
  (BoundingBox. (double x0) (double y0) (double x1) (double y1)))

(defn- table-options ^TableOptions [m]
  (let [b (TableOptions/builder)]
    (some->> (:vertical-strategy m) TableStrategy/parse (.verticalStrategy b))
    (some->> (:horizontal-strategy m) TableStrategy/parse (.horizontalStrategy b))
    (some->> (:snap-tolerance m) double (.snapTolerance b))
    (some->> (:snap-x-tolerance m) double (.snapXTolerance b))
    (some->> (:snap-y-tolerance m) double (.snapYTolerance b))
    (some->> (:join-tolerance m) double (.joinTolerance b))
    (some->> (:join-x-tolerance m) double (.joinXTolerance b))
    (some->> (:join-y-tolerance m) double (.joinYTolerance b))
    (some->> (:intersection-tolerance m) double (.intersectionTolerance b))
    (some->> (:intersection-x-tolerance m) double (.intersectionXTolerance b))
    (some->> (:intersection-y-tolerance m) double (.intersectionYTolerance b))
    (some->> (:explicit-vertical-lines m) (mapv double) (.explicitVerticalLines b))
    (some->> (:explicit-horizontal-lines m) (mapv double) (.explicitHorizontalLines b))
    (some->> (:crop m) bbox-of (.crop b))
    (some->> (:first-page-crop m) bbox-of (.firstPageCrop b))
    (some->> (:max-pages m) int (.maxPages b))
    (.build b)))

;; ---- results: Java values to the old map shapes ------------------------------------------

(defn- bbox [^BoundingBox b] {:x0 (.x0 b) :y0 (.y0 b) :x1 (.x1 b) :y1 (.y1 b)})
(defn- raw-bbox [^RawTableBoundingBox b] {:x0 (.x0 b) :top (.top b) :x1 (.x1 b) :bottom (.bottom b)})
(defn- some-vec [xs] (some-> xs vec))

(defn- layout-page [^LayoutPage p]
  {:page-number (.pageNumber p)
   :bbox        (bbox (.bbox p))
   :rotate      (.rotate p)
   :text        (.text p)
   :text-boxes
   (vec (for [^sa.ingenious.pdf.LayoutTextBox b (.toRecords p)]
          {:bbox (bbox (.bbox b)) :writing-mode (.writingMode b) :text (.text b)
           :lines (vec (for [^sa.ingenious.pdf.LayoutLine l (.lines b)]
                         {:bbox (bbox (.bbox l)) :orientation (.orientation l) :text (.text l)
                          :chars (vec (for [^sa.ingenious.pdf.LayoutChar c (.chars l)]
                                        {:text (.text c) :bbox (bbox (.bbox c))
                                         :font-name (.fontName c) :size (.size c)
                                         :upright (.upright c)}))}))}))})

(defn- raw-page [^RawPage p]
  (let [boxes (.boxes p)]
    {:page-index  (.pageIndex p)
     :page-number (.pageNumber p)
     :object-id   (.objectId p)
     :label       (.label p)
     :rotation    (.rotation p)
     :user-unit   (.userUnit p)
     :boxes       {:media (some-vec (.media boxes)) :crop (some-vec (.crop boxes))
                   :bleed (some-vec (.bleed boxes)) :trim (some-vec (.trim boxes))
                   :art (some-vec (.art boxes))}
     :layout-bbox (bbox (.layoutBbox p))
     :text        (.text p)
     :text-boxes
     (vec (for [^sa.ingenious.pdf.RawTextBox b (.rawTextBoxes p)]
            {:bbox (bbox (.bbox b)) :writing-mode (.writingMode b) :text (.text b)
             :lines (vec (for [^sa.ingenious.pdf.RawTextLine l (.lines b)]
                           {:bbox (bbox (.bbox l)) :orientation (.orientation l)
                            :raw-text (.rawText l) :text (.text l)
                            :characters
                            (vec (for [^sa.ingenious.pdf.RawCharacter c (.characters l)]
                                   {:text (.text c) :bbox (bbox (.bbox c)) :font-name (.fontName c)
                                    :size (.size c) :upright (.upright c) :advance (.advance c)
                                    :matrix (vec (.matrix c))
                                    :marked-content-id (.markedContentId c) :tag (.tag c)
                                    :non-stroking-color-space (.nonStrokingColorSpace c)
                                    :stroking-color-space (.strokingColorSpace c)
                                    :non-stroking-color (some-vec (.nonStrokingColor c))
                                    :stroking-color (some-vec (.strokingColor c))}))}))}))
     :tables
     (vec (for [^sa.ingenious.pdf.RawTable t (.tables p)]
            {:bbox (raw-bbox (.bbox t)) :row-count (.rowCount t) :column-count (.columnCount t)
             :cells (vec (for [^sa.ingenious.pdf.RawTableCell c (.cells t)]
                           {:row-index (.rowIndex c) :column-index (.columnIndex c)
                            :row-span (.rowSpan c) :column-span (.columnSpan c)
                            :bbox (raw-bbox (.bbox c)) :text (.text c)}))}))}))

(defn- page-text [^PageText p]
  {:page-number (.pageNumber p)
   :bbox (bbox (.bbox p))
   :text-boxes (vec (for [^sa.ingenious.pdf.PageTextBox b (.textBoxes p)]
                      {:bbox (bbox (.bbox b)) :text (.text b) :text-without-bidi (.textWithoutBidi b)
                       :lines (vec (for [^sa.ingenious.pdf.PageTextLine l (.lines b)]
                                     {:bbox (bbox (.bbox l)) :text (.text l)
                                      :text-without-bidi (.textWithoutBidi l)}))}))
   :fonts (set (.fonts p))
   :digit-styles (into #{} (map (fn [^sa.ingenious.pdf.DigitStyle s] [(.fontName s) (long (.size s))]))
                       (.digitStyles p))})

(defn- summary [^PageSummary s]
  {:page-number (.pageNumber s) :text (.text s) :bbox (bbox (.bbox s)) :rotate (.rotate s)})

(defn- table [^Table t]
  {:page-number (.pageNumber t) :bbox (bbox (.bbox t)) :row-count (.rowCount t)
   :column-count (.columnCount t)
   :cells (vec (for [^sa.ingenious.pdf.TableCell c (.cells t)]
                 {:row-index (.rowIndex c) :column-index (.columnIndex c) :row-span (.rowSpan c)
                  :column-span (.columnSpan c) :bbox (bbox (.bbox c)) :text (.text c)}))})

(defn- table-rows [^PageTableRows r]
  {:page-number (.pageNumber r) :cells (vec (.cells r))
   :row-offsets (vec (.rowOffsets r)) :table-offsets (vec (.tableOffsets r))})

(defn- metadata [^Metadata m]
  (let [v (.version m) p (.permissions m)]
    {:document-info (into {} (map (fn [^MetadataEntry e] [(.key e) (.value e)])) (.documentInfo m))
     :title (.title m) :author (.author m) :subject (.subject m) :keywords (.keywords m)
     :creator (.creator m) :producer (.producer m)
     :creation-date-raw (.creationDateRaw m) :creation-date-iso (.creationDateIso m)
     :modification-date-raw (.modificationDateRaw m) :modification-date-iso (.modificationDateIso m)
     :version {:header (.header v) :catalog (.catalog v) :effective (.effective v)}
     :file-size-bytes (.fileSizeBytes m) :page-count (.pageCount m) :encrypted (.encrypted m)
     :permissions {:printable (.printable p) :modifiable (.modifiable p) :extractable (.extractable p)}
     :linearized (.linearized m) :tagged (.tagged m) :user-properties (.userProperties m)
     :suspects (.suspects m) :form (.form m) :has-javascript (.hasJavascript m)
     :has-metadata-stream (.hasMetadataStream m) :xmp-metadata (.xmpMetadata m)}))

(defn- drain [^Cursor c f]
  (with-open [c c] (mapv f (iterator-seq c))))

;; ---- errors ------------------------------------------------------------------------------

(defn- old-error->class
  "The PdfException class the new API raises where the old one failed."
  [{:keys [error cause]}]
  (cond
    (= error :pdf/invalid-options) "InvalidArgument"
    cause cause
    :else (str error)))

(defn- error-class [^Throwable e]
  (if (or (instance? PdfException e) (not (instance? IllegalArgumentException e)))
    (.getSimpleName (class e))
    ;; A value the Java types cannot even express (an unknown strategy name).
    "InvalidArgument"))

(defn- attempt [f]
  (try (f) (catch Exception e {:error-class (error-class e)})))

(defn- normalise [expected]
  (if (and (map? expected) (contains? expected :error) (contains? expected :cause))
    {:error-class (old-error->class expected)}
    expected))

;; ---- replay ------------------------------------------------------------------------------

(defn- doc-calls [^Document d]
  {:text           #(.text d)
   :layout-pages   #(drain (.pages d) layout-page)
   :raw-document   #(let [pages (drain (.rawPages d) raw-page)]
                      {:declared-page-count (.declaredPageCount d)
                       :page-count (count pages)
                       :pages pages})
   :raw-page-1     #(raw-page (.rawPage d 1))
   :page-texts     #(drain (.pageTexts d) page-text)
   :page-summaries #(drain (.pageSummaries d) summary)
   :metadata       #(metadata (.metadata d))})

(defn- replay [{:keys [path opts tables]} expected]
  (let [path (str corpus/repo "/" path)]
    (try
      (with-open [d (Document/open ^String path (options opts))]
        (let [calls (doc-calls d)]
          (merge
           (into {} (for [k (keys (dissoc expected :tables-by-opts :open))]
                      [k (attempt (calls k))]))
           {:tables-by-opts
            (mapv (fn [t]
                    (into {} (for [[k f] {:tables #(drain (.tables d (table-options t)) table)
                                          :table-rows #(drain (.tableRows d (table-options t)) table-rows)}]
                               [k (attempt f)])))
                  tables)})))
      (catch Exception e {:open {:error-class (error-class e)}}))))

(defn- normalise-results [results]
  (cond-> (into {} (map (fn [[k v]] [k (normalise v)])) results)
    (:tables-by-opts results)
    (update :tables-by-opts
            (fn [ts] (mapv #(into {} (map (fn [[k v]] [k (normalise v)])) %) ts)))))

(deftest corpus-matches
  (let [files (corpus/files)]
    (is (< 25 (count files)) "the corpus is present")
    (doseq [file files]
      (let [{:keys [case results]} (corpus/read-case file)
            expected (normalise-results results)
            actual (replay case results)]
        (testing (:id case)
          (doseq [k (into (sorted-set) (concat (keys expected) (keys actual)))]
            (is (= (get expected k) (get actual k)) (str (:id case) " " k))))))))
