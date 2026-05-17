(ns sa.ingenious.pdf-test
  (:require [clojure.test :refer [deftest is run-tests testing]]
            [sa.ingenious.pdf :as pdf])
  (:import [clojure.lang ExceptionInfo]
           [java.util Arrays]
           [sa.ingenious.pdf BoundingBox DocumentOptions LayoutChar LayoutLine LayoutOptions
            LayoutPage LayoutTextBox PageSummary Table TableCell]))

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

(defn -main [& _]
  (let [{:keys [fail error]} (run-tests 'sa.ingenious.pdf-test)]
    (when (pos? (+ fail error))
      (System/exit 1))))
