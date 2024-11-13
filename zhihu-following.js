// 知乎following 
async function extractData() {
    const items = document.querySelectorAll('.List-item');
    const data = [];
    items.forEach(item => {
      const nameElement = item.querySelector('.UserLink-link');
      const name = nameElement.innerText;
      const url = nameElement.href;
      data.push({ name, url });
    });
    return data;
  }
  
  async function clickNextPage(accumulatedData) {
    const nextPageButton = document.querySelector('.PaginationButton-next');
    if (nextPageButton) {
      nextPageButton.click();
      await new Promise(resolve => setTimeout(resolve, 2000)); // 等待页面加载
      const newData = await extractData();
      accumulatedData.push(...newData);
      await clickNextPage(accumulatedData);
    } else {
      console.log(JSON.stringify(accumulatedData, null, 2));
    }
  }
  
  async function start() {
    const initialData = await extractData();
    await clickNextPage(initialData);
  }
  
  start();