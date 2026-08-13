(ns sa.ingenious.pdf-test
  (:require [clojure.test :refer [deftest is run-tests testing]]
            [sa.ingenious.pdf :as pdf])
  (:import [clojure.lang ExceptionInfo]
           [java.lang AutoCloseable]
           [java.util Arrays Iterator]
           [sa.ingenious.pdf BoundingBox DocumentOptions LayoutChar LayoutLine LayoutOptions
            LayoutPage LayoutTextBox MetadataEntry PageSummary PageTableRows PdfPermissions
            PdfVersion RawCharacter RawDocument RawDocumentMetadata RawPage RawPageBoxes RawTable
            RawTableBoundingBox RawTableCell RawTextBox RawTextLine Table TableCell TableOptions]))

(defn- one-list [value]
  (Arrays/asList (object-array [value])))

(deftest public-models-convert-to-idiomatic-clojure-data
  (let [bbox (BoundingBox. 1.0 2.0 3.0 4.0)
        summary (PageSummary. 1 "hello" bbox 0.0)
        layout-char (LayoutChar. "h" bbox "Helvetica" 12.0 true)
        line (LayoutLine. bbox "horizontal" "hello" (one-list layout-char))
        text-box (LayoutTextBox. bbox "lr-tb" "hello" (one-list line))
        layout-page (LayoutPage. 1 bbox 0.0 "hello" (one-list text-box))
        cell (TableCell. 0 0 1 1 bbox "hello")
        table (Table. 1 bbox 1 1 (one-list cell))]
    (is (= {:page-number 1
            :text "hello"
            :bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
            :rotate 0.0}
           (#'pdf/page-summary->map summary)))
    (is (= {:page-number 1
            :bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
            :rotate 0.0
            :text "hello"
            :text-boxes [{:bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                          :writing-mode "lr-tb"
                          :text "hello"
                          :lines [{:bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                                   :orientation "horizontal"
                                   :text "hello"
                                   :chars [{:text "h"
                                            :bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                                            :font-name "Helvetica"
                                            :size 12.0
                                            :upright true}]}]}]}
           (#'pdf/layout-page->map layout-page)))
    (is (= {:page-number 1
            :bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
            :row-count 1
            :column-count 1
            :cells [{:row-index 0
                     :column-index 0
                     :row-span 1
                     :column-span 1
                     :bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                     :text "hello"}]}
           (#'pdf/table->map table)))))

(deftest option-maps-convert-to-jvm-options
  (let [options (#'pdf/->document-options
                 {:password "secret"
                  :pages [1 2]
                  :max-pages 2
                  :caching false
                  :bidi true
                  :layout {:line-overlap 0.6
                           :char-margin 2.5
                           :line-margin 0.7
                           :word-margin 0.2
                           :boxes-flow nil
                           :detect-vertical true
                           :all-texts true}})
        layout (.layout options)]
    (is (instance? DocumentOptions options))
    (is (instance? LayoutOptions layout))
    (is (= "secret" (.password options)))
    (is (= [1 2] (vec (.pageNumbers options))))
    (is (= 2 (.maxPages options)))
    (is (false? (.caching options)))
    (is (true? (.bidi options)))
    (is (= 0.6 (.lineOverlap layout)))
    (is (= 2.5 (.charMargin layout)))
    (is (= 0.7 (.lineMargin layout)))
    (is (= 0.2 (.wordMargin layout)))
    (is (nil? (.boxesFlow layout)))
    (is (true? (.detectVertical layout)))
    (is (true? (.allTexts layout)))))

(deftest invalid-inputs-raise-ex-info
  (testing "unknown option"
    (is (thrown-with-msg?
         ExceptionInfo
         #"Unknown option"
         (#'pdf/->document-options {:bogus true}))))
  (testing "invalid page number"
    (is (thrown-with-msg?
         ExceptionInfo
         #"Page numbers"
         (#'pdf/->document-options {:pages [0]}))))
  (testing "invalid layout option"
    (is (thrown-with-msg?
         ExceptionInfo
         #"boxes-flow"
         (#'pdf/->document-options {:layout {:boxes-flow 2.0}}))))
  (testing "unsupported source"
    (is (thrown-with-msg?
         ExceptionInfo
         #"Unsupported source"
         (pdf/open (Object.))))))

(deftest jvm-failures-are-wrapped-as-ex-info
  (try
    (#'pdf/wrap-jvm-errors #(throw (IllegalStateException. "text failed")))
    (is false "expected ex-info")
    (catch ExceptionInfo ex
      (is (= :pdf/jvm-error (:type (ex-data ex))))
      (is (= "text failed" (.getMessage (.getCause ex)))))))

(deftest table-options-build-from-idiomatic-maps
  (testing "every supported key maps onto the builder"
    (let [^TableOptions options
          (#'pdf/->table-options
           {:vertical-strategy        "explicit"
            :horizontal-strategy      "text"
            :snap-tolerance           3
            :snap-x-tolerance         4
            :snap-y-tolerance         1
            :join-tolerance           20
            :intersection-x-tolerance 63
            :explicit-vertical-lines  [20 50 150]
            :crop                     [0 25 842 575]
            :first-page-crop          [0 15 842 575]
            :max-pages                10})]
      (is (= "explicit" (.verticalStrategy options)))
      (is (= "text" (.horizontalStrategy options)))
      (is (= 3.0 (.snapTolerance options)))
      (is (= 4.0 (.snapXTolerance options)))
      (is (= 1.0 (.snapYTolerance options)))
      (is (= 20.0 (.joinTolerance options)))
      (is (= 63.0 (.intersectionXTolerance options)))
      (is (nil? (.intersectionYTolerance options)))
      (is (= [20.0 50.0 150.0] (vec (.explicitVerticalLines options))))
      (is (= (BoundingBox. 0.0 25.0 842.0 575.0) (.crop options)))
      (is (= (BoundingBox. 0.0 15.0 842.0 575.0) (.firstPageCrop options)))
      (is (= 10 (.maxPages options)))))
  (testing "nil options mean defaults"
    (is (nil? (#'pdf/->table-options nil))))
  (testing "unknown keys are rejected"
    (is (thrown-with-msg?
         ExceptionInfo
         #"Unknown table option"
         (#'pdf/->table-options {:snap-tolerence 3}))))
  (testing "strategies are validated"
    (is (thrown-with-msg?
         ExceptionInfo
         #"vertical-strategy"
         (#'pdf/->table-options {:vertical-strategy "diagonal"}))))
  (testing "crops must be 4-number vectors"
    (is (thrown-with-msg?
         ExceptionInfo
         #"x0 y0 x1 y1"
         (#'pdf/->table-options {:crop [1 2 3]})))))

(deftest page-table-rows-convert-to-idiomatic-clojure-data
  (let [page (PageTableRows. 3
                             ["a" nil nil "b"]
                             (int-array [0 2 4])
                             (int-array [0 2]))]
    (is (= {:page-number 3
            :tables      [[["a" nil] [nil "b"]]]}
           (#'pdf/page-table-rows->map page)))))

(deftest mapped-cursor-is-lazy-and-closeable
  (let [values (atom [1])
        next-calls (atom 0)
        closed (atom false)
        source (reify
                 Iterator
                 (hasNext [_] (boolean (seq @values)))
                 (next [_]
                   (swap! next-calls inc)
                   (let [value (first @values)]
                     (swap! values subvec 1)
                     value))

                 AutoCloseable
                 (close [_] (reset! closed true)))]
    (with-open [cursor (#'pdf/mapped-cursor source inc)]
      (is (zero? @next-calls))
      (is (= [2] (vec cursor)))
      (is (= 1 @next-calls)))
    (is @closed)))

(deftest raw-document-converts-to-lossless-clojure-data
  (let [bbox (BoundingBox. 1.0 2.0 3.0 4.0)
        table-bbox (RawTableBoundingBox. 10.0 20.0 30.0 40.0)
        character (RawCharacter. "A" bbox "Helvetica" 12.0 true 7.5
                                 [1.0 0.0 0.0 1.0 20.0 30.0]
                                 (int 17) "Span" "DeviceRGB" "DeviceGray"
                                 [0.1 0.2 0.3] [0.4])
        line (RawTextLine. bbox "horizontal" "A" "A" (one-list character))
        text-box (RawTextBox. bbox "lr-tb" "A" (one-list line))
        cell (RawTableCell. (int 0) (int 0) (int 1) (int 1) table-bbox "value")
        table (RawTable. table-bbox (int 1) (int 1) (one-list cell))
        boxes (RawPageBoxes. [0.0 0.0 200.0 300.0] nil nil nil nil)
        page (RawPage. (int 0) (int 1) (int 42) "iv" (long 90) 2.0 boxes bbox "A"
                       (one-list text-box) (one-list table))
        document (RawDocument. (int 1) (int 1) (one-list page))]
    (is (= {:declared-page-count 1
            :page-count 1
            :pages [{:page-index 0
                     :page-number 1
                     :object-id 42
                     :label "iv"
                     :rotation 90
                     :user-unit 2.0
                     :boxes {:media [0.0 0.0 200.0 300.0]
                             :crop nil
                             :bleed nil
                             :trim nil
                             :art nil}
                     :layout-bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                     :text "A"
                     :text-boxes [{:bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                                   :writing-mode "lr-tb"
                                   :text "A"
                                   :lines [{:bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                                            :orientation "horizontal"
                                            :raw-text "A"
                                            :text "A"
                                            :characters [{:text "A"
                                                          :bbox {:x0 1.0 :y0 2.0 :x1 3.0 :y1 4.0}
                                                          :font-name "Helvetica"
                                                          :size 12.0
                                                          :upright true
                                                          :advance 7.5
                                                          :matrix [1.0 0.0 0.0 1.0 20.0 30.0]
                                                          :marked-content-id 17
                                                          :tag "Span"
                                                          :non-stroking-color-space "DeviceRGB"
                                                          :stroking-color-space "DeviceGray"
                                                          :non-stroking-color [0.1 0.2 0.3]
                                                          :stroking-color [0.4]}]}]}]
                     :tables [{:bbox {:x0 10.0 :top 20.0 :x1 30.0 :bottom 40.0}
                               :row-count 1
                               :column-count 1
                               :cells [{:row-index 0
                                        :column-index 0
                                        :row-span 1
                                        :column-span 1
                                        :bbox {:x0 10.0 :top 20.0 :x1 30.0 :bottom 40.0}
                                        :text "value"}]}]}]}
           (#'pdf/raw-document->map document)))))

(deftest metadata-converts-to-canonical-clojure-data
  (let [metadata (RawDocumentMetadata.
                  [(MetadataEntry. "Title" "Example Document")
                   (MetadataEntry. "CustomField" "custom value")]
                  "Example Document" "Example Author" nil nil "Octagon" "iText"
                  "D:20260704114317+03'00'" "2026-07-04T11:43:17+03:00"
                  nil nil
                  (PdfVersion. "1.4" "1.7" "1.7")
                  (long 1225686) (int 91) false
                  (PdfPermissions. true true true)
                  false true true false "acroform" true true "<x:xmpmeta/>")]
    (is (= {:document-info {"Title" "Example Document"
                            "CustomField" "custom value"}
            :title "Example Document"
            :author "Example Author"
            :subject nil
            :keywords nil
            :creator "Octagon"
            :producer "iText"
            :creation-date-raw "D:20260704114317+03'00'"
            :creation-date-iso "2026-07-04T11:43:17+03:00"
            :modification-date-raw nil
            :modification-date-iso nil
            :version {:header "1.4" :catalog "1.7" :effective "1.7"}
            :file-size-bytes 1225686
            :page-count 91
            :encrypted false
            :permissions {:printable true :modifiable true :extractable true}
            :linearized false
            :tagged true
            :user-properties true
            :suspects false
            :form "acroform"
            :has-javascript true
            :has-metadata-stream true
            :xmp-metadata "<x:xmpmeta/>"}
           (#'pdf/raw-document-metadata->map metadata)))))

(defn -main [& _]
  (let [{:keys [fail error]} (run-tests 'sa.ingenious.pdf-test)]
    (when (pos? (+ fail error))
      (System/exit 1))))
