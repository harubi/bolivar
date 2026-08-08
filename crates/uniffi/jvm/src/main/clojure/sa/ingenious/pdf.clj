(ns sa.ingenious.pdf
  (:import [clojure.lang ExceptionInfo]
           [java.io File InputStream]
           [java.nio.file Path]
           [sa.ingenious.pdf BoundingBox Document DocumentOptions LayoutOptions PdfException
            PageTableRows TableOptions]))

(def ^:private byte-array-class (Class/forName "[B"))
(def ^:private option-keys #{:password :pages :max-pages :caching :layout})
(def ^:private layout-keys
  #{:line-overlap :char-margin :line-margin :word-margin :boxes-flow :detect-vertical :all-texts})
(def ^:private table-keys
  #{:vertical-strategy :horizontal-strategy
    :snap-tolerance :snap-x-tolerance :snap-y-tolerance
    :join-tolerance :join-x-tolerance :join-y-tolerance
    :intersection-tolerance :intersection-x-tolerance :intersection-y-tolerance
    :explicit-vertical-lines :explicit-horizontal-lines
    :crop :first-page-crop :max-pages})
(def ^:private table-strategies #{"lines" "lines_strict" "text" "explicit"})

(defn- fail!
  ([message data]
   (throw (ex-info message data)))
  ([message data cause]
   (throw (ex-info message data cause))))

(defn- assert-map! [value label]
  (when-not (or (nil? value) (map? value))
    (fail! (str label " must be a map") {:type  :pdf/invalid-options
                                         :value value})))

(defn- assert-known-keys! [value allowed label]
  (let [unknown (seq (remove allowed (keys value)))]
    (when unknown
      (fail! (str "Unknown " label (if (= 1 (count unknown)) " key: " " keys: ")
                  (pr-str (vec unknown)))
             {:type         :pdf/invalid-options
              :unknown-keys (vec unknown)}))))

(defn- assert-boolean! [value key]
  (when-not (boolean? value)
    (fail! (str key " must be true or false") {:type  :pdf/invalid-options
                                               :key   key
                                               :value value})))

(defn- assert-positive-int! [value message key]
  (when-not (and (integer? value) (pos? value))
    (fail! message {:type  :pdf/invalid-options
                    :key   key
                    :value value})))

(defn- assert-number! [value key]
  (when-not (number? value)
    (fail! (str key " must be a number") {:type  :pdf/invalid-options
                                          :key   key
                                          :value value})))

(defn- ->layout-options [opts]
  (assert-map! opts ":layout")
  (when opts
    (assert-known-keys! opts layout-keys "layout option")
    (let [builder (LayoutOptions/builder)]
      (when (contains? opts :line-overlap)
        (assert-number! (:line-overlap opts) ":line-overlap")
        (.lineOverlap builder (double (:line-overlap opts))))
      (when (contains? opts :char-margin)
        (assert-number! (:char-margin opts) ":char-margin")
        (.charMargin builder (double (:char-margin opts))))
      (when (contains? opts :line-margin)
        (assert-number! (:line-margin opts) ":line-margin")
        (.lineMargin builder (double (:line-margin opts))))
      (when (contains? opts :word-margin)
        (assert-number! (:word-margin opts) ":word-margin")
        (.wordMargin builder (double (:word-margin opts))))
      (when (contains? opts :boxes-flow)
        (let [boxes-flow (:boxes-flow opts)]
          (when (some? boxes-flow)
            (assert-number! boxes-flow ":boxes-flow")
            (when-not (<= -1.0 (double boxes-flow) 1.0)
              (fail! ":boxes-flow must be nil or within [-1.0, 1.0]"
                     {:type  :pdf/invalid-options
                      :key   :boxes-flow
                      :value boxes-flow})))
          (.boxesFlow builder (when (some? boxes-flow) (Double/valueOf (double boxes-flow))))))
      (when (contains? opts :detect-vertical)
        (assert-boolean! (:detect-vertical opts) ":detect-vertical")
        (.detectVertical builder (:detect-vertical opts)))
      (when (contains? opts :all-texts)
        (assert-boolean! (:all-texts opts) ":all-texts")
        (.allTexts builder (:all-texts opts)))
      (.build builder))))

(defn- ->document-options [opts]
  (assert-map! opts "options")
  (if-not opts
    (DocumentOptions.)
    (do
      (assert-known-keys! opts option-keys "option")
      (let [builder (DocumentOptions/builder)]
        (when (contains? opts :password)
          (let [password (:password opts)]
            (when-not (or (nil? password) (string? password))
              (fail! ":password must be a string or nil" {:type  :pdf/invalid-options
                                                          :key   :password
                                                          :value password}))
            (.password builder password)))
        (when (contains? opts :pages)
          (let [pages (:pages opts)]
            (when-not (sequential? pages)
              (fail! ":pages must be a sequence of 1-based page numbers"
                     {:type  :pdf/invalid-options
                      :key   :pages
                      :value pages}))
            (doseq [page pages]
              (assert-positive-int! page "Page numbers are 1-based and must be positive" :pages))
            (.pageNumbers builder (mapv #(Integer/valueOf (int %)) pages))))
        (when (contains? opts :max-pages)
          (let [max-pages (:max-pages opts)]
            (assert-positive-int! max-pages ":max-pages must be a positive integer" :max-pages)
            (.maxPages builder (Integer/valueOf (int max-pages)))))
        (when (contains? opts :caching)
          (assert-boolean! (:caching opts) ":caching")
          (.caching builder (:caching opts)))
        (when (contains? opts :layout)
          (.layout builder (->layout-options (:layout opts))))
        (.build builder)))))

(defn- byte-array? [value]
  (instance? byte-array-class value))

(defn- wrap-jvm-errors [f]
  (try
    (f)
    (catch ExceptionInfo ex
      (throw ex))
    (catch PdfException ex
      (fail! (.getMessage ex) {:type :pdf/native-error} ex))
    (catch Exception ex
      (fail! (.getMessage ex) {:type :pdf/jvm-error} ex))))

(defn- bbox->map [^BoundingBox bbox]
  {:x0 (.x0 bbox)
   :y0 (.y0 bbox)
   :x1 (.x1 bbox)
   :y1 (.y1 bbox)})

(declare layout-line->map)

(defn- layout-char->map [ch]
  {:text      (.text ch)
   :bbox      (bbox->map (.bbox ch))
   :font-name (.fontName ch)
   :size      (.size ch)
   :upright   (.upright ch)})

(defn- layout-line->map [line]
  {:bbox        (bbox->map (.bbox line))
   :orientation (.orientation line)
   :text        (.text line)
   :chars       (mapv layout-char->map (.chars line))})

(defn- layout-text-box->map [text-box]
  {:bbox         (bbox->map (.bbox text-box))
   :writing-mode (.writingMode text-box)
   :text         (.text text-box)
   :lines        (mapv layout-line->map (.lines text-box))})

(defn- page-summary->map [summary]
  {:page-number (.pageNumber summary)
   :text        (.text summary)
   :bbox        (bbox->map (.bbox summary))
   :rotate      (.rotate summary)})

(defn- layout-page->map [page]
  {:page-number (.pageNumber page)
   :bbox        (bbox->map (.bbox page))
   :rotate      (.rotate page)
   :text        (.text page)
   :text-boxes  (mapv layout-text-box->map (.textBoxes page))})

(defn- table-cell->map [cell]
  {:row-index    (.rowIndex cell)
   :column-index (.columnIndex cell)
   :row-span     (.rowSpan cell)
   :column-span  (.columnSpan cell)
   :bbox         (bbox->map (.bbox cell))
   :text         (.text cell)})

(defn- table->map [table]
  {:page-number  (.pageNumber table)
   :bbox         (bbox->map (.bbox table))
   :row-count    (.rowCount table)
   :column-count (.columnCount table)
   :cells        (mapv table-cell->map (.cells table))})

(defn- raw-table-bbox->map [bbox]
  {:x0 (.x0 bbox)
   :top (.top bbox)
   :x1 (.x1 bbox)
   :bottom (.bottom bbox)})

(defn- raw-character->map [character]
  {:text (.text character)
   :bbox (bbox->map (.bbox character))
   :font-name (.fontName character)
   :size (.size character)
   :upright (.upright character)
   :advance (.advance character)
   :matrix (vec (.matrix character))
   :marked-content-id (.markedContentId character)
   :tag (.tag character)
   :non-stroking-color-space (.nonStrokingColorSpace character)
   :stroking-color-space (.strokingColorSpace character)
   :non-stroking-color (some-> (.nonStrokingColor character) vec)
   :stroking-color (some-> (.strokingColor character) vec)})

(defn- raw-text-line->map [line]
  {:bbox (bbox->map (.bbox line))
   :orientation (.orientation line)
   :text (.text line)
   :characters (mapv raw-character->map (.characters line))})

(defn- raw-text-box->map [text-box]
  {:bbox (bbox->map (.bbox text-box))
   :writing-mode (.writingMode text-box)
   :text (.text text-box)
   :lines (mapv raw-text-line->map (.lines text-box))})

(defn- raw-table-cell->map [cell]
  {:row-index (.rowIndex cell)
   :column-index (.columnIndex cell)
   :row-span (.rowSpan cell)
   :column-span (.columnSpan cell)
   :bbox (raw-table-bbox->map (.bbox cell))
   :text (.text cell)})

(defn- raw-table->map [table]
  {:bbox (raw-table-bbox->map (.bbox table))
   :row-count (.rowCount table)
   :column-count (.columnCount table)
   :cells (mapv raw-table-cell->map (.cells table))})

(defn- raw-page-boxes->map [boxes]
  {:media (some-> (.media boxes) vec)
   :crop (some-> (.crop boxes) vec)
   :bleed (some-> (.bleed boxes) vec)
   :trim (some-> (.trim boxes) vec)
   :art (some-> (.art boxes) vec)})

(defn- raw-page->map [page]
  {:page-index (.pageIndex page)
   :page-number (.pageNumber page)
   :object-id (.objectId page)
   :label (.label page)
   :rotation (.rotation page)
   :user-unit (.userUnit page)
   :boxes (raw-page-boxes->map (.boxes page))
   :layout-bbox (bbox->map (.layoutBbox page))
   :text (.text page)
   :text-boxes (mapv raw-text-box->map (.textBoxes page))
   :tables (mapv raw-table->map (.tables page))})

(defn- raw-document->map [document]
  {:declared-page-count (.declaredPageCount document)
   :page-count (.pageCount document)
   :pages (mapv raw-page->map (.pages document))})

(defn- raw-document-metadata->map [metadata]
  {:document-info (into {}
                        (map (juxt #(.key %) #(.value %)))
                        (.documentInfo metadata))
   :title (.title metadata)
   :author (.author metadata)
   :subject (.subject metadata)
   :keywords (.keywords metadata)
   :creator (.creator metadata)
   :producer (.producer metadata)
   :creation-date-raw (.creationDateRaw metadata)
   :creation-date-iso (.creationDateIso metadata)
   :modification-date-raw (.modificationDateRaw metadata)
   :modification-date-iso (.modificationDateIso metadata)
   :version {:header (.header (.version metadata))
             :catalog (.catalog (.version metadata))
             :effective (.effective (.version metadata))}
   :file-size-bytes (.fileSizeBytes metadata)
   :page-count (.pageCount metadata)
   :encrypted (.encrypted metadata)
   :permissions {:printable (.printable (.permissions metadata))
                 :modifiable (.modifiable (.permissions metadata))
                 :extractable (.extractable (.permissions metadata))}
   :linearized (.linearized metadata)
   :tagged (.tagged metadata)
   :user-properties (.userProperties metadata)
   :suspects (.suspects metadata)
   :form (.form metadata)
   :has-javascript (.hasJavascript metadata)
   :has-metadata-stream (.hasMetadataStream metadata)
   :xmp-metadata (.xmpMetadata metadata)})

(defn open
  "Open a PDF source and return an AutoCloseable document handle.

  Source may be a path string, java.nio.file.Path, java.io.File, java.io.InputStream,
  or byte array. Use with-open when managing the document directly."
  ([source]
   (open source nil))
  ([source opts]
   (wrap-jvm-errors
    #(let [options (->document-options opts)]
       (cond
         (string? source) (Document/open ^String source options)
         (instance? Path source) (Document/open ^Path source options)
         (instance? File source) (Document/open ^File source options)
         (instance? InputStream source) (Document/open ^InputStream source options)
         (byte-array? source) (Document/open ^bytes source options)
         :else (fail! "Unsupported source; expected string, Path, File, InputStream, or byte array"
                      {:type         :pdf/unsupported-source
                       :source-class (some-> source class .getName)}))))))

(defn text [^Document doc]
  (wrap-jvm-errors #(.extractText doc)))

(defn page-summaries [^Document doc]
  (wrap-jvm-errors #(mapv page-summary->map (.extractPageSummaries doc))))

(defn layout-pages [^Document doc]
  (wrap-jvm-errors #(mapv layout-page->map (.extractLayoutPages doc))))

(defn raw-document
  "Extract the lossless page, character, and raw table graph."
  [^Document doc]
  (wrap-jvm-errors #(raw-document->map (.extractRawDocument doc))))

(defn raw-page
  "Extract one 1-based page as the lossless page, character, and table graph."
  [^Document doc page-number]
  (wrap-jvm-errors #(raw-page->map (.extractRawPage doc (int page-number)))))

(defn metadata
  "Return canonical and raw PDF document metadata."
  [^Document doc]
  (wrap-jvm-errors #(raw-document-metadata->map (.metadata doc))))

(defn version
  "Return the Bolivar library version."
  []
  (wrap-jvm-errors #(Document/version)))

(defn- assert-strategy! [value key]
  (when-not (contains? table-strategies value)
    (fail! (str key " must be one of " (pr-str (sort table-strategies)))
           {:type :pdf/invalid-options
            :key key
            :value value})))

(defn- assert-crop! [value key]
  (when-not (and (sequential? value)
                 (= 4 (count value))
                 (every? number? value))
    (fail! (str key " must be [x0 y0 x1 y1]") {:type :pdf/invalid-options
                                               :key key
                                               :value value})))

(defn- ->crop ^BoundingBox [[x0 y0 x1 y1]]
  (BoundingBox. (double x0) (double y0) (double x1) (double y1)))

(defn- ->table-options ^TableOptions [opts]
  (assert-map! opts "table options")
  (when opts
    (assert-known-keys! opts table-keys "table option")
    (let [builder (TableOptions/builder)]
      (doseq [[key apply!] [[:vertical-strategy #(.verticalStrategy builder %)]
                            [:horizontal-strategy #(.horizontalStrategy builder %)]]
              :let [value (get opts key)]
              :when (some? value)]
        (assert-strategy! value key)
        (apply! value))
      (doseq [[key apply!] [[:snap-tolerance #(.snapTolerance builder %)]
                            [:snap-x-tolerance #(.snapXTolerance builder %)]
                            [:snap-y-tolerance #(.snapYTolerance builder %)]
                            [:join-tolerance #(.joinTolerance builder %)]
                            [:join-x-tolerance #(.joinXTolerance builder %)]
                            [:join-y-tolerance #(.joinYTolerance builder %)]
                            [:intersection-tolerance #(.intersectionTolerance builder %)]
                            [:intersection-x-tolerance #(.intersectionXTolerance builder %)]
                            [:intersection-y-tolerance #(.intersectionYTolerance builder %)]]
              :let [value (get opts key)]
              :when (some? value)]
        (assert-number! value key)
        (apply! (Double/valueOf (double value))))
      (doseq [[key apply!] [[:explicit-vertical-lines #(.explicitVerticalLines builder %)]
                            [:explicit-horizontal-lines #(.explicitHorizontalLines builder %)]]
              :let [value (get opts key)]
              :when (some? value)]
        (when-not (and (sequential? value) (every? number? value))
          (fail! (str key " must be a sequence of numbers")
                 {:type :pdf/invalid-options
                  :key key
                  :value value}))
        (apply! (mapv #(Double/valueOf (double %)) value)))
      (when-some [max-pages (:max-pages opts)]
        (assert-positive-int! max-pages ":max-pages must be a positive integer" :max-pages)
        (.maxPages builder (Integer/valueOf (int max-pages))))
      (doseq [[key apply!] [[:crop #(.crop builder %)]
                            [:first-page-crop #(.firstPageCrop builder %)]]
              :let [value (get opts key)]
              :when (some? value)]
        (assert-crop! value key)
        (apply! (->crop value)))
      (.build builder))))

(defn tables
  "Extract tables. `opts` tunes extraction (pdfplumber vocabulary):
  :vertical-strategy/:horizontal-strategy (\"lines\" \"lines_strict\" \"text\"
  \"explicit\"), :snap-tolerance/:join-tolerance/:intersection-tolerance and
  their -x-/-y- variants, :explicit-vertical-lines/:explicit-horizontal-lines,
  and page crops :crop/:first-page-crop as [x0 y0 x1 y1]."
  ([^Document doc]
   (wrap-jvm-errors #(mapv table->map (.extractTables doc))))
  ([^Document doc opts]
   (if (nil? opts)
     (tables doc)
     (wrap-jvm-errors
      #(mapv table->map (.extractTables doc (->table-options opts)))))))

(defn- page-table-rows->map [^PageTableRows page]
  {:page-number (.pageNumber page)
   :tables (mapv (fn [table] (mapv vec table)) (.tables page))})

(defn table-rows
  "Extract raw table rows per page (nil = empty cell), exactly as the
  pdfplumber-compatible rows pipeline emits them. Takes the same options as
  `tables`. Returns [{:page-number n :tables [[[cell ...] ...] ...]}]."
  ([^Document doc]
   (table-rows doc nil))
  ([^Document doc opts]
   (wrap-jvm-errors
    #(mapv page-table-rows->map (.extractTableRows doc (->table-options opts))))))

(defn extract-text
  ([source]
   (extract-text source nil))
  ([source opts]
   (if (instance? Document source)
     (do
       (when (some? opts)
         (fail! "Options cannot be provided when source is already a Document"
                {:type :pdf/invalid-options
                 :key  :options}))
       (text source))
     (with-open [doc (open source opts)]
       (text doc)))))
